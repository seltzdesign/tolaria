//! Tauri commands backing the GitHub Projects Settings UI and bind-project
//! modal. The PAT is loaded from the OS keychain by the commands that need
//! to call GitHub; the renderer never sees it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;
use serde_yaml::Value as YamlValue;

use crate::github::projects::{
    auth,
    binding::{self, GithubBindingInput},
    client::{self, ClientConfig, ClientError},
    connection,
    push::{self, PushInput, PushPlan},
    queries::{ProjectField, ProjectSummary},
    snapshot::{self, ProjectSnapshot, SnapshotItem},
    sync::{self, PullInput, PullSummary},
    url::{parse_project_url, ProjectOwner},
};

#[tauri::command]
pub fn github_set_pat(pat: String) -> Result<(), String> {
    auth::store_pat(&pat)
}

#[tauri::command]
pub fn github_clear_pat() -> Result<(), String> {
    auth::delete_pat()
}

#[tauri::command]
pub fn github_pat_present() -> bool {
    auth::pat_present()
}

#[tauri::command]
pub fn github_test_connection() -> Result<String, String> {
    connection::test_connection()
}

#[derive(Debug, Serialize)]
pub struct ProjectResolution {
    pub project: ProjectSummary,
    pub fields: Vec<ProjectField>,
}

/// Resolves a Project URL to its node id + field schema. The Bind modal
/// uses this to populate the field-mapping UI before the user commits.
#[tauri::command]
pub async fn github_resolve_project_url(project_url: String) -> Result<ProjectResolution, String> {
    let parsed = parse_project_url(&project_url)?;
    let pat = auth::load_pat()?
        .ok_or_else(|| "No GitHub personal access token is stored.".to_string())?;
    let http = client::build_http_client().map_err(stringify)?;
    let config = ClientConfig::new(pat);

    let projects = match parsed.owner {
        ProjectOwner::User => client::list_projects_for_user(&http, &config, &parsed.login).await,
        ProjectOwner::Org => client::list_projects_for_org(&http, &config, &parsed.login).await,
    }
    .map_err(stringify)?;

    let project = projects
        .into_iter()
        .find(|candidate| candidate.number == parsed.number as i32)
        .ok_or_else(|| {
            format!(
                "Project #{} not found under {}.",
                parsed.number, parsed.login
            )
        })?;
    let fields = client::get_project_fields(&http, &config, &project.id)
        .await
        .map_err(stringify)?;
    Ok(ProjectResolution { project, fields })
}

#[tauri::command]
pub fn github_bind_project(
    note_path: String,
    binding_input: GithubBindingInput,
) -> Result<(), String> {
    binding::write_binding(Path::new(&note_path), &binding_input)
}

#[tauri::command]
pub fn github_unbind_project(note_path: String) -> Result<(), String> {
    binding::clear_binding(Path::new(&note_path))
}

/// Counts returned to the renderer from a manual sync cycle. Carries
/// both the pull-side numbers (what changed locally because of remote
/// state) and the push-side numbers (what we sent upstream) so the
/// renderer can show a single result line covering the round trip.
#[derive(Debug, Serialize)]
pub struct SyncResult {
    // Pull
    pub created: u32,
    pub updated: u32,
    pub deleted: u32,
    pub unchanged: u32,
    pub items_seen: u32,
    pub items_skipped: u32,
    // Push
    pub pushed_creates: u32,
    pub pushed_field_updates: u32,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// One round trip against GitHub for a single bound project. Pulls
/// remote items down into the vault, then walks the bound `task_folder`
/// and pushes local draft creates / field updates back up. The snapshot
/// store is what makes "is this a real change?" a cheap question on
/// both sides — pull writes it at the end of its pass, push reads it
/// before planning, and any successful mutations stamp the new state
/// back so the next cycle doesn't re-push.
#[tauri::command]
pub async fn github_sync(vault_path: String, note_path: String) -> Result<SyncResult, String> {
    let vault_path = PathBuf::from(crate::commands::expand_tilde(&vault_path).into_owned());
    let note_path = Path::new(&note_path).to_path_buf();
    let binding = read_project_binding(&note_path)?;
    let pat = auth::load_pat()?
        .ok_or_else(|| "No GitHub personal access token is stored.".to_string())?;
    let http = client::build_http_client().map_err(stringify)?;
    let config = ClientConfig::new(pat);
    let now = Utc::now().to_rfc3339();
    let project_node_id = binding.project_node_id.clone();
    let cache_base = snapshot::default_base();
    let project_note_stem = filename_stem(&note_path);

    // === Pull ===
    let items = client::list_all_project_items(&http, &config, &project_node_id)
        .await
        .map_err(stringify)?;
    let items_seen = items.len() as u32;
    let items_skipped = items.iter().filter(|i| i.content.is_none()).count() as u32;
    let pull_summary = sync::pull(&PullInput {
        vault_path: vault_path.clone(),
        task_folder_rel: binding.task_folder_rel.clone(),
        project_node_id: project_node_id.clone(),
        project_note_stem,
        status_field: binding.status_field.clone(),
        field_mappings: binding.field_mappings.clone(),
        items,
        now_rfc3339: now.clone(),
        cache_base: cache_base.clone(),
    })?;

    // === Push ===
    let field_schema = client::get_project_fields(&http, &config, &project_node_id)
        .await
        .map_err(stringify)?;
    let mut snap = snapshot::load(&cache_base, &project_node_id);
    let plan = push::plan_push(
        &PushInput {
            vault_path: vault_path.clone(),
            task_folder_rel: binding.task_folder_rel.clone(),
            project_node_id: project_node_id.clone(),
            status_field: binding.status_field.clone(),
            field_mappings: binding.field_mappings.clone(),
            field_schema,
            now_rfc3339: now.clone(),
        },
        &snap.items,
    )?;

    let mut warnings = plan.warnings.clone();
    let mut errors: Vec<String> = Vec::new();
    let mut pushed_creates = 0u32;
    let mut pushed_field_updates = 0u32;

    let ctx = ExecCtx {
        http: &http,
        config: &config,
        project_node_id: &project_node_id,
        vault_path: &vault_path,
        now_rfc3339: &now,
    };
    pushed_creates += execute_creates(
        &ctx,
        &plan,
        &mut snap,
        &mut errors,
        &mut pushed_field_updates,
    )
    .await;
    pushed_field_updates += execute_updates(&ctx, &plan, &mut snap, &mut errors).await;

    snap.synced_at = now.clone();
    snapshot::save(&cache_base, &snap)?;

    append_sync_log(&vault_path, &project_node_id, &now, &pull_summary);
    let mut combined_errors = pull_summary.errors;
    combined_errors.append(&mut errors);
    warnings.dedup();
    Ok(SyncResult {
        created: pull_summary.created,
        updated: pull_summary.updated,
        deleted: pull_summary.deleted,
        unchanged: pull_summary.unchanged,
        items_seen,
        items_skipped,
        pushed_creates,
        pushed_field_updates,
        warnings,
        errors: combined_errors,
    })
}

/// Shared inputs for the push-executor halves — bundled so neither
/// function trips clippy's `too_many_arguments` lint and so they stay
/// symmetric. Borrowed because the executor's lifetime is the whole
/// `github_sync` call.
struct ExecCtx<'a> {
    http: &'a reqwest::Client,
    config: &'a ClientConfig,
    project_node_id: &'a str,
    vault_path: &'a Path,
    now_rfc3339: &'a str,
}

async fn execute_creates(
    ctx: &ExecCtx<'_>,
    plan: &PushPlan,
    snap: &mut ProjectSnapshot,
    errors: &mut Vec<String>,
    pushed_field_updates: &mut u32,
) -> u32 {
    let mut count = 0u32;
    for create in &plan.creates {
        let new_item_id = match client::add_draft_issue(
            ctx.http,
            ctx.config,
            ctx.project_node_id,
            &create.title,
            create.body.as_deref(),
        )
        .await
        {
            Ok(id) => id,
            Err(e) => {
                errors.push(format!("create draft `{}`: {e}", create.title));
                continue;
            }
        };
        let abs = ctx.vault_path.join(&create.local_file_path);
        if let Err(e) = push::write_back_create_metadata(
            &abs,
            ctx.project_node_id,
            &new_item_id,
            ctx.now_rfc3339,
        ) {
            errors.push(e);
        }
        let mut item_field_values = std::collections::BTreeMap::new();
        for field in &create.follow_up_fields {
            match client::update_project_item_field(
                ctx.http,
                ctx.config,
                ctx.project_node_id,
                &new_item_id,
                &field.field_id,
                field.value.clone(),
            )
            .await
            {
                Ok(_) => {
                    item_field_values
                        .insert(field.field_name.clone(), field.remote_value_str.clone());
                    *pushed_field_updates += 1;
                }
                Err(e) => errors.push(format!(
                    "set `{}` on new item `{}`: {e}",
                    field.field_name, create.title
                )),
            }
        }
        snap.items.insert(
            new_item_id.clone(),
            SnapshotItem {
                item_id: new_item_id,
                content_type: "DraftIssue".into(),
                title: create.title.clone(),
                body: create.body.clone(),
                url: None,
                number: None,
                repository: None,
                field_values: item_field_values,
                local_file_path: create.local_file_path.clone(),
            },
        );
        count += 1;
    }
    count
}

async fn execute_updates(
    ctx: &ExecCtx<'_>,
    plan: &PushPlan,
    snap: &mut ProjectSnapshot,
    errors: &mut Vec<String>,
) -> u32 {
    let mut count = 0u32;
    for update in &plan.updates {
        match client::update_project_item_field(
            ctx.http,
            ctx.config,
            ctx.project_node_id,
            &update.item_id,
            &update.field_id,
            update.value.clone(),
        )
        .await
        {
            Ok(_) => {
                if let Some(item) = snap.items.get_mut(&update.item_id) {
                    item.field_values
                        .insert(update.field_name.clone(), update.remote_value_str.clone());
                }
                count += 1;
            }
            Err(e) => errors.push(format!(
                "update `{}` on `{}`: {e}",
                update.field_name, update.local_file_path
            )),
        }
    }
    count
}

/// Slice of the project-note binding the sync engine needs to do its work.
/// Extracted here so we can keep the Tauri command body short and so the
/// (small) frontmatter-reading logic stays close to the command that uses it.
struct ProjectBinding {
    project_node_id: String,
    task_folder_rel: String,
    status_field: Option<String>,
    field_mappings: Vec<(String, String)>,
}

fn read_project_binding(note_path: &Path) -> Result<ProjectBinding, String> {
    let content = fs::read_to_string(note_path)
        .map_err(|e| format!("Failed to read project note `{}`: {e}", note_path.display()))?;
    let frontmatter = parse_frontmatter(&content)?;
    let project_node_id = required_string(&frontmatter, "github_project_node_id")?;
    let task_folder_rel = required_string(&frontmatter, "task_folder")?;
    let status_field = optional_string(&frontmatter, "status_field");
    let field_mappings = read_field_mappings(&frontmatter);
    Ok(ProjectBinding {
        project_node_id,
        task_folder_rel,
        status_field,
        field_mappings,
    })
}

fn parse_frontmatter(content: &str) -> Result<YamlValue, String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok(YamlValue::Mapping(Default::default()));
    }
    let opener = content.find("---").expect("just checked the prefix");
    let after_open = &content[opener + 3..];
    let after_open = after_open.trim_start_matches('\r').trim_start_matches('\n');
    let close = after_open
        .find("\n---")
        .ok_or_else(|| "Project note frontmatter is unterminated.".to_string())?;
    let fm_text = &after_open[..close];
    if fm_text.trim().is_empty() {
        return Ok(YamlValue::Mapping(Default::default()));
    }
    serde_yaml::from_str(fm_text)
        .map_err(|e| format!("Failed to parse project note frontmatter: {e}"))
}

fn required_string(value: &YamlValue, key: &str) -> Result<String, String> {
    optional_string(value, key)
        .ok_or_else(|| format!("Project note is missing required `{key}` frontmatter."))
}

fn optional_string(value: &YamlValue, key: &str) -> Option<String> {
    value
        .as_mapping()
        .and_then(|m| m.get(YamlValue::String(key.to_string())))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

fn read_field_mappings(value: &YamlValue) -> Vec<(String, String)> {
    let Some(mapping) = value
        .as_mapping()
        .and_then(|m| m.get(YamlValue::String("field_mappings".to_string())))
        .and_then(|v| v.as_mapping())
    else {
        return Vec::new();
    };
    mapping
        .iter()
        .filter_map(|(k, v)| Some((k.as_str()?.to_string(), v.as_str()?.to_string())))
        .collect()
}

fn filename_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("project")
        .to_string()
}

/// JSONL-formatted, append-only sync log inside the vault. One line per
/// pull cycle so it's easy to tail; errors are *logged* here and otherwise
/// non-fatal so a missing `.laputa/` directory can't tank a sync the user
/// just confirmed worked.
fn append_sync_log(
    vault_path: &Path,
    project_node_id: &str,
    timestamp: &str,
    summary: &PullSummary,
) {
    let log_dir = vault_path.join(".laputa");
    if fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    let entry = serde_json::json!({
        "timestamp": timestamp,
        "project_node_id": project_node_id,
        "op": "pull",
        "result": {
            "created": summary.created,
            "updated": summary.updated,
            "deleted": summary.deleted,
            "unchanged": summary.unchanged,
        },
        "errors": summary.errors,
    });
    let line = match serde_json::to_string(&entry) {
        Ok(s) => s,
        Err(_) => return,
    };
    let path = log_dir.join("sync-log.jsonl");
    let Ok(mut handle) = fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(handle, "{line}");
}

fn stringify(error: ClientError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn note_with_frontmatter(dir: &TempDir, body: &str) -> PathBuf {
        let path = dir.path().join("q2-launch.md");
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn read_project_binding_returns_full_struct_with_field_mappings() {
        let dir = TempDir::new().unwrap();
        let path = note_with_frontmatter(
            &dir,
            "---\ntype: project\ntask_folder: tasks/q2\ngithub_project_node_id: PVT_kw_demo\nstatus_field: Status\nfield_mappings:\n  priority: Priority\n  due: \"End date\"\n---\n# Q2 Launch\n",
        );
        let binding = read_project_binding(&path).unwrap();
        assert_eq!(binding.project_node_id, "PVT_kw_demo");
        assert_eq!(binding.task_folder_rel, "tasks/q2");
        assert_eq!(binding.status_field.as_deref(), Some("Status"));
        assert!(binding
            .field_mappings
            .contains(&("priority".to_string(), "Priority".to_string())));
        assert!(binding
            .field_mappings
            .contains(&("due".to_string(), "End date".to_string())));
    }

    #[test]
    fn read_project_binding_fails_without_node_id() {
        let dir = TempDir::new().unwrap();
        let path = note_with_frontmatter(&dir, "---\ntype: project\ntask_folder: tasks\n---\n");
        let err = read_project_binding(&path).err().expect("expected error");
        assert!(err.contains("github_project_node_id"));
    }

    #[test]
    fn read_project_binding_fails_without_task_folder() {
        let dir = TempDir::new().unwrap();
        let path = note_with_frontmatter(
            &dir,
            "---\ntype: project\ngithub_project_node_id: PVT_demo\n---\n",
        );
        let err = read_project_binding(&path).err().expect("expected error");
        assert!(err.contains("task_folder"));
    }

    #[test]
    fn append_sync_log_writes_one_jsonl_line_per_cycle() {
        let dir = TempDir::new().unwrap();
        let summary = PullSummary {
            created: 3,
            updated: 1,
            deleted: 0,
            unchanged: 4,
            errors: vec![],
        };
        append_sync_log(dir.path(), "PVT_kw_demo", "2026-05-15T12:00:00Z", &summary);
        append_sync_log(dir.path(), "PVT_kw_demo", "2026-05-15T12:05:00Z", &summary);
        let log = fs::read_to_string(dir.path().join(".laputa/sync-log.jsonl")).unwrap();
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("PVT_kw_demo"));
        assert!(lines[0].contains("\"created\":3"));
    }
}
