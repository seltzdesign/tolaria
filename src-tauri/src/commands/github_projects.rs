//! Tauri commands backing the GitHub Projects Settings UI.

use crate::github::projects::{auth, connection};

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
