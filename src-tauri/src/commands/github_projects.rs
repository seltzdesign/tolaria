//! Tauri commands backing the GitHub Projects Settings UI and bind-project
//! modal. The PAT is loaded from the OS keychain by the commands that need
//! to call GitHub; the renderer never sees it.

use std::path::Path;

use serde::Serialize;

use crate::github::projects::{
    auth,
    binding::{self, GithubBindingInput},
    client::{self, ClientConfig, ClientError},
    connection,
    queries::{ProjectField, ProjectSummary},
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

fn stringify(error: ClientError) -> String {
    error.to_string()
}
