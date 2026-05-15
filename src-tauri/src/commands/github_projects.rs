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
    queries::{ProjectField, ProjectSummary},
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

/// Counts returned to the renderer from a manual pull. Mirrors
/// `sync::PullSummary` but keeps the wire shape stable independent of the
/// internal struct so we can extend it without breaking the frontend.
#[derive(Debug, Serialize)]
pub struct PullResult {
    pub created: u32,
    pub updated: u32,
    pub deleted: u32,
    pub unchanged: u32,
    pub errors: Vec<String>,
}

/// Manual pull for a single bound project. Reads the binding from the
/// project note's frontmatter, fetches every remote item, asks the sync
/// engine to apply diffs to the vault, and appends one line to the
/// per-vault sync log so we have an audit trail of every cycle.
#[tauri::command]
pub async fn github_sync_pull(vault_path: String, note_path: String) -> Result<PullResult, String> {
    let vault_path = PathBuf::from(crate::commands::expand_tilde(&vault_path).into_owned());
    let note_path = Path::new(&note_path).to_path_buf();
    let binding = read_project_binding(&note_path)?;
    let pat = auth::load_pat()?
        .ok_or_else(|| "No GitHub personal access token is stored.".to_string())?;
    let http = client::build_http_client().map_err(stringify)?;
    let config = ClientConfig::new(pat);
    let items = client::list_all_project_items(&http, &config, &binding.project_node_id)
        .await
        .map_err(stringify)?;
    let now = Utc::now().to_rfc3339();
    let project_note_stem = filename_stem(&note_path);
    let project_node_id = binding.project_node_id.clone();
    let summary = sync::pull(&PullInput {
        vault_path: vault_path.clone(),
        task_folder_rel: binding.task_folder_rel,
        project_node_id: project_node_id.clone(),
        project_note_stem,
        status_field: binding.status_field,
        field_mappings: binding.field_mappings,
        items,
        now_rfc3339: now.clone(),
        cache_base: crate::github::projects::snapshot::default_base(),
    })?;
    append_sync_log(&vault_path, &project_node_id, &now, &summary);
    Ok(PullResult {
        created: summary.created,
        updated: summary.updated,
        deleted: summary.deleted,
        unchanged: summary.unchanged,
        errors: summary.errors,
    })
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
