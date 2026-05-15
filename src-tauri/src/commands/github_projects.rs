//! Tauri commands backing the GitHub Projects Settings UI, the bind-project
//! modal, and the bridge sync runtime. The PAT is loaded from the OS
//! keychain by the commands that need to call GitHub; the renderer never
//! sees it.
//!
//! All non-trivial logic lives in the `github::projects` submodules; this
//! file is just the Tauri command surface that delegates to them. The two
//! sync entry points (`github_sync` and the scheduler controls) both call
//! into `scheduler::run_sync_for_project` so manual clicks and background
//! ticks share one orchestration.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::github::projects::{
    auth,
    binding::{self, GithubBindingInput},
    client::{self, ClientConfig, ClientError},
    connection,
    queries::{ProjectField, ProjectSummary},
    scheduler::{self, SchedulerState, SyncResult, SyncTrigger},
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

/// One manual round trip against GitHub for the project at `note_path`.
/// Pull first, then push. The shared `run_sync_for_project` orchestration
/// emits lifecycle events (`github_sync_started`, `github_sync_finished`,
/// `github_sync_error`) so the renderer can drive spinners and reload
/// open editor tabs whose paths were just rewritten.
#[tauri::command]
pub async fn github_sync(
    app: AppHandle,
    vault_path: String,
    note_path: String,
) -> Result<SyncResult, String> {
    let vault_path = PathBuf::from(crate::commands::expand_tilde(&vault_path).into_owned());
    let note_path = Path::new(&note_path).to_path_buf();
    scheduler::run_sync_for_project(&app, &vault_path, &note_path, SyncTrigger::Manual).await
}

/// Start or refresh the background scheduler for `vault_path`. Idempotent:
/// re-invoking with the same vault reconciles the running set against the
/// current bindings on disk (so the renderer can re-call this after a
/// bind / unbind / settings change without worrying about duplicates).
#[tauri::command]
pub fn github_scheduler_start(
    state: State<'_, SchedulerState>,
    app: AppHandle,
    vault_path: String,
) -> Result<(), String> {
    let vault_path = PathBuf::from(crate::commands::expand_tilde(&vault_path).into_owned());
    scheduler::start_or_refresh(&state, &app, &vault_path)
}

/// Stop every running scheduler task. Used when the user switches
/// vaults or closes the app — anything that would make the live tasks
/// point at the wrong filesystem.
#[tauri::command]
pub fn github_scheduler_stop(state: State<'_, SchedulerState>) -> Result<(), String> {
    state.stop_all();
    Ok(())
}

fn stringify(error: ClientError) -> String {
    error.to_string()
}
