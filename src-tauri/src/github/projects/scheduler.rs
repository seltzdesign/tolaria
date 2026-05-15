//! Background scheduler for the GitHub Projects v2 bridge.
//!
//! Spawns one Tokio task per bound project with `sync_enabled: true`,
//! ticking on the project's configured `sync_interval_minutes`. Each tick
//! invokes the same orchestration the manual Sync button uses, emits
//! lifecycle events to the renderer, and updates the snapshot.
//!
//! ## Discovery
//!
//! The scheduler doesn't watch the vault reactively. It scans on demand
//! via [`start_or_refresh`], which the renderer invokes:
//! - once at app start, and
//! - again after any bind / unbind / settings change.
//!
//! Reactive vault watching is appealing but adds significant moving
//! parts (file-watcher debouncing, partial-write detection, cross-task
//! coherence) for a feature that's already gated on the renderer
//! triggering reconfiguration. Manual refresh is good enough until the
//! UI grows the kind of "set & forget" surface that would benefit.
//!
//! ## Race fix (P14)
//!
//! Each sync cycle returns the vault-relative paths it touched.
//! `run_sync_for_project` emits a `github_sync_finished` Tauri event
//! carrying those paths so the renderer can force-reload any open
//! editor tab — that's how we keep autosave from overwriting changes
//! the sync just landed on disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use super::auth;
use super::client::{self, ClientConfig};
use super::push::{self, PushInput};
use super::snapshot::{self, ProjectSnapshot, SnapshotItem};
use super::sync::{self, PullInput, PullSummary};

const DEFAULT_INTERVAL_MINUTES: u32 = 5;

/// Why this sync cycle ran. Tagged on every lifecycle event so the
/// renderer can distinguish "I just clicked the button" from "the
/// scheduler woke up on its interval" — useful for spinners + analytics.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncTrigger {
    Manual,
    Scheduled,
}

/// Wire-shape returned to the renderer from a manual sync, and emitted
/// inside `github_sync_finished` Tauri events. Defined here (not in
/// `commands::github_projects`) because both the command and the
/// scheduler return it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncResult {
    // Pull
    pub created: u32,
    pub updated: u32,
    pub deleted: u32,
    pub unchanged: u32,
    pub items_seen: u32,
    pub items_skipped: u32,
    pub conflicts: u32,
    // Push
    pub pushed_creates: u32,
    pub pushed_field_updates: u32,
    /// Vault-relative paths the cycle touched. Renderer iterates this
    /// to reload any open editor tab whose path appears.
    pub touched_paths: Vec<String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Manage the live set of project-sync Tokio tasks. Stored as Tauri
/// app state. Cancel signals are `Notify` rather than `AbortHandle`
/// because we want a clean shutdown — finish the in-flight HTTP work,
/// don't yank a half-applied write.
#[derive(Default)]
pub struct SchedulerState {
    inner: Mutex<SchedulerInner>,
}

#[derive(Default)]
struct SchedulerInner {
    tasks: HashMap<String, RunningTask>,
}

struct RunningTask {
    note_path: PathBuf,
    interval_minutes: u32,
    handle: JoinHandle<()>,
    cancel: Arc<Notify>,
}

impl SchedulerState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stop every running task. Awaits each Tokio handle so the next
    /// `start_or_refresh` can't observe stale entries.
    pub fn stop_all(&self) {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        for (_, task) in inner.tasks.drain() {
            task.cancel.notify_one();
            task.handle.abort();
        }
    }
}

/// Scan the vault for bound projects with `sync_enabled: true` and
/// reconcile the live task set against it. Idempotent: re-running with
/// the same vault is a no-op except for projects whose interval
/// changed (those get restarted).
pub fn start_or_refresh(
    state: &SchedulerState,
    app: &AppHandle,
    vault_path: &Path,
) -> Result<(), String> {
    let projects = discover_bound_projects(vault_path)?;
    let mut inner = state
        .inner
        .lock()
        .map_err(|_| "scheduler state poisoned".to_string())?;

    let live_ids: std::collections::HashSet<String> =
        projects.iter().map(|p| p.project_node_id.clone()).collect();
    let stale_keys: Vec<String> = inner
        .tasks
        .keys()
        .filter(|k| !live_ids.contains(*k))
        .cloned()
        .collect();
    for key in stale_keys {
        if let Some(task) = inner.tasks.remove(&key) {
            task.cancel.notify_one();
            task.handle.abort();
        }
    }

    for project in projects {
        let needs_restart = inner
            .tasks
            .get(&project.project_node_id)
            .map(|existing| {
                existing.interval_minutes != project.interval_minutes
                    || existing.note_path != project.note_path
            })
            .unwrap_or(true);
        if !needs_restart {
            continue;
        }
        if let Some(old) = inner.tasks.remove(&project.project_node_id) {
            old.cancel.notify_one();
            old.handle.abort();
        }
        let cancel = Arc::new(Notify::new());
        let handle = spawn_project_loop(
            app.clone(),
            vault_path.to_path_buf(),
            project.clone(),
            cancel.clone(),
        );
        inner.tasks.insert(
            project.project_node_id.clone(),
            RunningTask {
                note_path: project.note_path,
                interval_minutes: project.interval_minutes,
                handle,
                cancel,
            },
        );
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct BoundProject {
    project_node_id: String,
    note_path: PathBuf,
    interval_minutes: u32,
}

/// Walk the vault, find every `type: project` note with a non-empty
/// `github_project_node_id` and `sync_enabled: true`, and emit a small
/// `BoundProject` per match. Files that fail to parse are skipped
/// quietly — they'll surface as user-facing errors via the manual sync
/// path if they're actually broken bindings.
fn discover_bound_projects(vault_path: &Path) -> Result<Vec<BoundProject>, String> {
    let mut paths: Vec<PathBuf> = Vec::new();
    walk_markdown(vault_path, &mut paths);
    let mut out: Vec<BoundProject> = Vec::new();
    for path in paths {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(frontmatter) = parse_frontmatter(&content) else {
            continue;
        };
        let mapping = match &frontmatter {
            serde_yaml::Value::Mapping(m) => m,
            _ => continue,
        };
        if mapping
            .get(serde_yaml::Value::String("type".into()))
            .and_then(|v| v.as_str())
            != Some("project")
        {
            continue;
        }
        let Some(project_node_id) = mapping
            .get(serde_yaml::Value::String("github_project_node_id".into()))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
        else {
            continue;
        };
        let sync_enabled = mapping
            .get(serde_yaml::Value::String("sync_enabled".into()))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !sync_enabled {
            continue;
        }
        let interval_minutes = mapping
            .get(serde_yaml::Value::String("sync_interval_minutes".into()))
            .and_then(|v| v.as_u64())
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_INTERVAL_MINUTES);
        out.push(BoundProject {
            project_node_id,
            note_path: path,
            interval_minutes,
        });
    }
    Ok(out)
}

fn walk_markdown(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') {
                continue;
            }
        }
        if path.is_dir() {
            walk_markdown(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

fn parse_frontmatter(content: &str) -> Result<serde_yaml::Value, String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok(serde_yaml::Value::Mapping(Default::default()));
    }
    let opener = content.find("---").expect("just checked the prefix");
    let after_open = &content[opener + 3..];
    let after_open = after_open.trim_start_matches('\r').trim_start_matches('\n');
    let close = after_open
        .find("\n---")
        .ok_or_else(|| "frontmatter unterminated".to_string())?;
    let fm_text = &after_open[..close];
    if fm_text.trim().is_empty() {
        return Ok(serde_yaml::Value::Mapping(Default::default()));
    }
    serde_yaml::from_str(fm_text).map_err(|e| e.to_string())
}

fn spawn_project_loop(
    app: AppHandle,
    vault_path: PathBuf,
    project: BoundProject,
    cancel: Arc<Notify>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_secs(project.interval_minutes as u64 * 60);
        loop {
            tokio::select! {
                _ = cancel.notified() => break,
                _ = tokio::time::sleep(interval) => {}
            }
            let _ = run_sync_for_project(
                &app,
                &vault_path,
                &project.note_path,
                SyncTrigger::Scheduled,
            )
            .await;
        }
    })
}

/// Full pull + push cycle for one bound project. Shared by the manual
/// `github_sync` command and every scheduler tick. Emits lifecycle
/// events (`github_sync_started`, `github_sync_finished`,
/// `github_sync_error`) so the renderer can show spinners / refresh
/// open editor tabs after a successful sync.
pub async fn run_sync_for_project(
    app: &AppHandle,
    vault_path: &Path,
    note_path: &Path,
    trigger: SyncTrigger,
) -> Result<SyncResult, String> {
    let project_note_stem = note_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("project")
        .to_string();
    let binding = match read_project_binding(note_path) {
        Ok(b) => b,
        Err(e) => {
            emit_error(app, None, &trigger, &e);
            return Err(e);
        }
    };
    let project_node_id = binding.project_node_id.clone();
    let _ = app.emit(
        "github_sync_started",
        SyncLifecycleEvent {
            project_node_id: project_node_id.clone(),
            trigger,
        },
    );
    match do_sync(vault_path, note_path, &project_note_stem, &binding).await {
        Ok(result) => {
            let _ = app.emit(
                "github_sync_finished",
                SyncFinishedEvent {
                    project_node_id: project_node_id.clone(),
                    trigger,
                    result: result.clone(),
                },
            );
            Ok(result)
        }
        Err(e) => {
            emit_error(app, Some(&project_node_id), &trigger, &e);
            Err(e)
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct SyncLifecycleEvent {
    project_node_id: String,
    trigger: SyncTrigger,
}

#[derive(Debug, Clone, Serialize)]
struct SyncFinishedEvent {
    project_node_id: String,
    trigger: SyncTrigger,
    result: SyncResult,
}

#[derive(Debug, Clone, Serialize)]
struct SyncErrorEvent {
    project_node_id: Option<String>,
    trigger: SyncTrigger,
    message: String,
}

fn emit_error(app: &AppHandle, project_node_id: Option<&str>, trigger: &SyncTrigger, msg: &str) {
    let _ = app.emit(
        "github_sync_error",
        SyncErrorEvent {
            project_node_id: project_node_id.map(|s| s.to_string()),
            trigger: *trigger,
            message: msg.to_string(),
        },
    );
}

/// Project-note binding values the orchestration cares about. Loaded
/// once per cycle so a frontmatter change between ticks is picked up
/// without needing scheduler refresh.
struct ProjectBindingValues {
    project_node_id: String,
    task_folder_rel: String,
    status_field: Option<String>,
    field_mappings: Vec<(String, String)>,
}

fn read_project_binding(note_path: &Path) -> Result<ProjectBindingValues, String> {
    let content = std::fs::read_to_string(note_path)
        .map_err(|e| format!("Failed to read project note `{}`: {e}", note_path.display()))?;
    let frontmatter = parse_frontmatter(&content)?;
    let mapping = match &frontmatter {
        serde_yaml::Value::Mapping(m) => m.clone(),
        _ => return Err("Project note frontmatter is not a mapping.".into()),
    };
    let project_node_id = mapping
        .get(serde_yaml::Value::String("github_project_node_id".into()))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| "Project note is missing `github_project_node_id`.".to_string())?;
    let task_folder_rel = mapping
        .get(serde_yaml::Value::String("task_folder".into()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Project note is missing `task_folder`.".to_string())?;
    let status_field = mapping
        .get(serde_yaml::Value::String("status_field".into()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let field_mappings = mapping
        .get(serde_yaml::Value::String("field_mappings".into()))
        .and_then(|v| v.as_mapping())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.as_str()?.to_string(), v.as_str()?.to_string())))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(ProjectBindingValues {
        project_node_id,
        task_folder_rel,
        status_field,
        field_mappings,
    })
}

async fn do_sync(
    vault_path: &Path,
    _note_path: &Path,
    project_note_stem: &str,
    binding: &ProjectBindingValues,
) -> Result<SyncResult, String> {
    let pat = auth::load_pat()?
        .ok_or_else(|| "No GitHub personal access token is stored.".to_string())?;
    let http = client::build_http_client().map_err(|e| e.to_string())?;
    let config = ClientConfig::new(pat);
    let now = Utc::now().to_rfc3339();
    let cache_base = snapshot::default_base();
    let project_node_id = binding.project_node_id.clone();

    // === Pull ===
    let items = client::list_all_project_items(&http, &config, &project_node_id)
        .await
        .map_err(|e| e.to_string())?;
    let items_seen = items.len() as u32;
    let items_skipped = items.iter().filter(|i| i.content.is_none()).count() as u32;
    let pull_summary = sync::pull(&PullInput {
        vault_path: vault_path.to_path_buf(),
        task_folder_rel: binding.task_folder_rel.clone(),
        project_node_id: project_node_id.clone(),
        project_note_stem: project_note_stem.to_string(),
        status_field: binding.status_field.clone(),
        field_mappings: binding.field_mappings.clone(),
        items,
        now_rfc3339: now.clone(),
        cache_base: cache_base.clone(),
    })?;

    // === Push ===
    let field_schema = client::get_project_fields(&http, &config, &project_node_id)
        .await
        .map_err(|e| e.to_string())?;
    let mut snap = snapshot::load(&cache_base, &project_node_id);
    let plan = push::plan_push(
        &PushInput {
            vault_path: vault_path.to_path_buf(),
            task_folder_rel: binding.task_folder_rel.clone(),
            project_node_id: project_node_id.clone(),
            project_note_stem: project_note_stem.to_string(),
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
    let mut touched_paths = pull_summary.touched_paths.clone();

    pushed_creates += execute_creates(
        &http,
        &config,
        &project_node_id,
        vault_path,
        &now,
        &plan,
        &mut snap,
        &mut errors,
        &mut pushed_field_updates,
        &mut touched_paths,
    )
    .await;
    pushed_field_updates += execute_updates(
        &http,
        &config,
        &project_node_id,
        &plan,
        &mut snap,
        &mut errors,
    )
    .await;

    snap.synced_at = now.clone();
    snapshot::save(&cache_base, &snap)?;

    append_sync_log(vault_path, &project_node_id, &now, &pull_summary);
    let mut combined_errors = pull_summary.errors.clone();
    combined_errors.append(&mut errors);
    warnings.dedup();
    touched_paths.sort();
    touched_paths.dedup();
    Ok(SyncResult {
        created: pull_summary.created,
        updated: pull_summary.updated,
        deleted: pull_summary.deleted,
        unchanged: pull_summary.unchanged,
        items_seen,
        items_skipped,
        conflicts: pull_summary.conflicts,
        pushed_creates,
        pushed_field_updates,
        touched_paths,
        warnings,
        errors: combined_errors,
    })
}

#[allow(clippy::too_many_arguments)]
async fn execute_creates(
    http: &reqwest::Client,
    config: &ClientConfig,
    project_node_id: &str,
    vault_path: &Path,
    now_rfc3339: &str,
    plan: &push::PushPlan,
    snap: &mut ProjectSnapshot,
    errors: &mut Vec<String>,
    pushed_field_updates: &mut u32,
    touched_paths: &mut Vec<String>,
) -> u32 {
    let mut count = 0u32;
    for create in &plan.creates {
        let new_item_id = match client::add_draft_issue(
            http,
            config,
            project_node_id,
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
        let abs = vault_path.join(&create.local_file_path);
        if let Err(e) =
            push::write_back_create_metadata(&abs, project_node_id, &new_item_id, now_rfc3339)
        {
            errors.push(e);
        }
        touched_paths.push(create.local_file_path.clone());
        let mut item_field_values = std::collections::BTreeMap::new();
        for field in &create.follow_up_fields {
            match client::update_project_item_field(
                http,
                config,
                project_node_id,
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
                remote_updated_at: Some(now_rfc3339.to_string()),
                local_file_path: create.local_file_path.clone(),
            },
        );
        count += 1;
    }
    count
}

async fn execute_updates(
    http: &reqwest::Client,
    config: &ClientConfig,
    project_node_id: &str,
    plan: &push::PushPlan,
    snap: &mut ProjectSnapshot,
    errors: &mut Vec<String>,
) -> u32 {
    let mut count = 0u32;
    for update in &plan.updates {
        match client::update_project_item_field(
            http,
            config,
            project_node_id,
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

fn append_sync_log(
    vault_path: &Path,
    project_node_id: &str,
    timestamp: &str,
    summary: &PullSummary,
) {
    let log_dir = vault_path.join(".laputa");
    if std::fs::create_dir_all(&log_dir).is_err() {
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
            "conflicts": summary.conflicts,
        },
        "errors": summary.errors,
    });
    let line = match serde_json::to_string(&entry) {
        Ok(s) => s,
        Err(_) => return,
    };
    let path = log_dir.join("sync-log.jsonl");
    let Ok(mut handle) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    use std::io::Write;
    let _ = writeln!(handle, "{line}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_note(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn discover_finds_only_projects_with_sync_enabled_and_node_id() {
        let vault = TempDir::new().unwrap();
        write_note(
            vault.path(),
            "a.md",
            "---\ntype: project\ngithub_project_node_id: PVT_a\nsync_enabled: true\nsync_interval_minutes: 3\ntask_folder: tasks/a\n---\n",
        );
        write_note(
            vault.path(),
            "b.md",
            "---\ntype: project\ngithub_project_node_id: PVT_b\nsync_enabled: false\ntask_folder: tasks/b\n---\n",
        );
        write_note(
            vault.path(),
            "c.md",
            "---\ntype: project\ntask_folder: tasks/c\n---\n",
        );
        write_note(vault.path(), "d.md", "---\ntype: task\n---\n");
        let found = discover_bound_projects(vault.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].project_node_id, "PVT_a");
        assert_eq!(found[0].interval_minutes, 3);
    }

    #[test]
    fn discover_falls_back_to_default_interval_when_missing_or_zero() {
        let vault = TempDir::new().unwrap();
        write_note(
            vault.path(),
            "a.md",
            "---\ntype: project\ngithub_project_node_id: PVT_a\nsync_enabled: true\ntask_folder: tasks/a\n---\n",
        );
        write_note(
            vault.path(),
            "b.md",
            "---\ntype: project\ngithub_project_node_id: PVT_b\nsync_enabled: true\nsync_interval_minutes: 0\ntask_folder: tasks/b\n---\n",
        );
        let found = discover_bound_projects(vault.path()).unwrap();
        let intervals: Vec<u32> = found.iter().map(|p| p.interval_minutes).collect();
        assert!(intervals.iter().all(|n| *n == DEFAULT_INTERVAL_MINUTES));
    }

    #[test]
    fn discover_skips_files_that_fail_to_parse() {
        let vault = TempDir::new().unwrap();
        write_note(vault.path(), "broken.md", "---\nthis is not yaml: : : :\n");
        write_note(
            vault.path(),
            "good.md",
            "---\ntype: project\ngithub_project_node_id: PVT_good\nsync_enabled: true\ntask_folder: t\n---\n",
        );
        let found = discover_bound_projects(vault.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].project_node_id, "PVT_good");
    }

    #[test]
    fn read_project_binding_returns_full_struct_with_field_mappings() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("q2-launch.md");
        std::fs::write(
            &path,
            "---\ntype: project\ntask_folder: tasks/q2\ngithub_project_node_id: PVT_kw_demo\nstatus_field: Status\nfield_mappings:\n  priority: Priority\n  due: \"End date\"\n---\n# Q2 Launch\n",
        )
        .unwrap();
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
        let path = dir.path().join("project.md");
        std::fs::write(&path, "---\ntype: project\ntask_folder: tasks\n---\n").unwrap();
        let err = read_project_binding(&path).err().expect("expected error");
        assert!(err.contains("github_project_node_id"));
    }

    #[test]
    fn read_project_binding_fails_without_task_folder() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("project.md");
        std::fs::write(
            &path,
            "---\ntype: project\ngithub_project_node_id: PVT_demo\n---\n",
        )
        .unwrap();
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
            conflicts: 0,
            touched_paths: vec![],
            errors: vec![],
        };
        append_sync_log(dir.path(), "PVT_kw_demo", "2026-05-15T12:00:00Z", &summary);
        append_sync_log(dir.path(), "PVT_kw_demo", "2026-05-15T12:05:00Z", &summary);
        let log = std::fs::read_to_string(dir.path().join(".laputa/sync-log.jsonl")).unwrap();
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("PVT_kw_demo"));
        assert!(lines[0].contains("\"created\":3"));
    }

    #[test]
    fn discover_ignores_hidden_directories() {
        let vault = TempDir::new().unwrap();
        write_note(
            vault.path(),
            ".git/info/exclude.md",
            "---\ntype: project\ngithub_project_node_id: PVT_x\nsync_enabled: true\ntask_folder: t\n---\n",
        );
        let found = discover_bound_projects(vault.path()).unwrap();
        assert!(found.is_empty());
    }
}
