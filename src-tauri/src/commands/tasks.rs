//! Tauri commands for creating task and project notes.
//!
//! Thin wrappers around [`crate::vault::create_task_note`] and
//! [`crate::vault::create_project_note`]. The vault layer owns the validation,
//! collision resolution, and lazy type-doc seeding; this layer handles tilde
//! expansion and the Tauri command boundary.

use crate::vault::{self, CreateNoteResult};

use super::expand_tilde;

#[tauri::command]
pub fn create_task(
    vault_path: String,
    folder: String,
    title: String,
    project: Option<String>,
) -> Result<CreateNoteResult, String> {
    let vault_path = expand_tilde(&vault_path);
    vault::create_task_note(
        std::path::Path::new(vault_path.as_ref()),
        &folder,
        &title,
        project.as_deref(),
    )
}

#[tauri::command]
pub fn create_project(
    vault_path: String,
    folder: String,
    title: String,
) -> Result<CreateNoteResult, String> {
    let vault_path = expand_tilde(&vault_path);
    vault::create_project_note(
        std::path::Path::new(vault_path.as_ref()),
        &folder,
        &title,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn create_task_command_routes_through_vault_path_boundary() {
        let dir = TempDir::new().unwrap();
        let vault_path = dir.path().to_string_lossy().to_string();

        let result = create_task(
            vault_path,
            String::new(),
            "Hello".to_string(),
            Some("Q2".to_string()),
        )
        .unwrap();
        assert!(result.path.ends_with("hello.md"));
        assert!(dir.path().join("hello.md").exists());
        assert!(dir.path().join("task.md").exists());

        let content = std::fs::read_to_string(dir.path().join("hello.md")).unwrap();
        assert!(content.contains("project: \"[[Q2]]\""));
    }

    #[test]
    fn create_project_command_routes_through_vault_path_boundary() {
        let dir = TempDir::new().unwrap();
        let vault_path = dir.path().to_string_lossy().to_string();

        let result = create_project(vault_path, String::new(), "Launch".to_string()).unwrap();
        assert!(result.path.ends_with("launch.md"));
        assert!(dir.path().join("project.md").exists());

        let content = std::fs::read_to_string(dir.path().join("launch.md")).unwrap();
        assert!(content.contains("type: project"));
        assert!(content.contains("launch/tasks"));
    }
}
