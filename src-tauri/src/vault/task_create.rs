//! Creation helpers for task and project notes.
//!
//! Per [ADR 0115 §1](../../../docs/adr/0115-tasks-and-projects-as-typed-notes.md), task
//! and project notes are regular `VaultEntry` markdown files with a locked frontmatter
//! schema. These helpers compose the right frontmatter, resolve filename collisions
//! per [ADR 0007](../../../docs/adr/0007-title-filename-sync.md), and lazy-seed the
//! starter type documents at vault root the first time the feature is used.

use serde::Serialize;
use std::path::{Path, PathBuf};

use super::file::create_note_content;
use super::filename_rules::validate_filename_stem;
use super::rename::title_to_slug;

const STARTER_TASK_TYPE_DOC: &str = include_str!("../../resources/starter-types/task.md");
const STARTER_PROJECT_TYPE_DOC: &str = include_str!("../../resources/starter-types/project.md");

/// Result of creating a task or project note.
#[derive(Debug, Serialize, Clone)]
pub struct CreateNoteResult {
    pub path: String,
    pub warnings: Vec<String>,
}

/// Create a new task `.md` file. The new note has `type: task` frontmatter, an H1
/// matching the title, and an optional `project` wikilink. All other task fields are
/// left blank for the editor to fill in.
pub fn create_task_note(
    vault_path: &Path,
    folder: &str,
    title: &str,
    project: Option<&str>,
) -> Result<CreateNoteResult, String> {
    let mut warnings = Vec::new();
    if let Some(warning) = seed_type_doc_if_missing(vault_path, "task", STARTER_TASK_TYPE_DOC)? {
        warnings.push(warning);
    }
    let target_folder = resolve_target_folder(vault_path, folder)?;
    let path = unique_path_for_title(&target_folder, title)?;
    let content = render_task_body(title, project);
    create_note_content(&path.to_string_lossy(), &content)?;
    Ok(CreateNoteResult {
        path: path.to_string_lossy().into_owned(),
        warnings,
    })
}

/// Create a new project `.md` file. The new note has `type: project` frontmatter, an
/// H1 matching the title, and a default `task_folder` pointing at its own directory.
pub fn create_project_note(
    vault_path: &Path,
    folder: &str,
    title: &str,
) -> Result<CreateNoteResult, String> {
    let mut warnings = Vec::new();
    if let Some(warning) =
        seed_type_doc_if_missing(vault_path, "project", STARTER_PROJECT_TYPE_DOC)?
    {
        warnings.push(warning);
    }
    let target_folder = resolve_target_folder(vault_path, folder)?;
    let path = unique_path_for_title(&target_folder, title)?;
    let task_folder = default_task_folder(vault_path, &target_folder, title);
    let content = render_project_body(title, &task_folder);
    create_note_content(&path.to_string_lossy(), &content)?;
    Ok(CreateNoteResult {
        path: path.to_string_lossy().into_owned(),
        warnings,
    })
}

/// Lazy-seed `<vault>/<slug>.md` with the starter type doc when no type document for
/// `slug` exists at either vault root or `type/<slug>.md`. Returns a one-line warning
/// when a seed happened so the caller can surface it. Returns `None` when no seed was
/// needed.
fn seed_type_doc_if_missing(
    vault_path: &Path,
    slug: &str,
    body: &str,
) -> Result<Option<String>, String> {
    let root_path = vault_path.join(format!("{slug}.md"));
    let type_dir_path = vault_path.join("type").join(format!("{slug}.md"));
    if root_path.exists() || type_dir_path.exists() {
        return Ok(None);
    }
    create_note_content(&root_path.to_string_lossy(), body)?;
    Ok(Some(format!(
        "seeded {slug}.md type document at vault root"
    )))
}

fn resolve_target_folder(vault_path: &Path, folder: &str) -> Result<PathBuf, String> {
    let trimmed = folder
        .trim()
        .trim_start_matches('/')
        .trim_start_matches('\\');
    let target = if trimmed.is_empty() {
        vault_path.to_path_buf()
    } else {
        vault_path.join(trimmed)
    };
    Ok(target)
}

fn unique_path_for_title(folder: &Path, title: &str) -> Result<PathBuf, String> {
    let stem = title_to_slug(title);
    validate_filename_stem(&stem)?;
    let candidate = folder.join(format!("{stem}.md"));
    if !candidate.exists() {
        return Ok(candidate);
    }
    for suffix in 2..=999u32 {
        let candidate = folder.join(format!("{stem}-{suffix}.md"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "Could not find an unused filename for '{title}' after 999 attempts"
    ))
}

fn default_task_folder(vault_path: &Path, project_folder: &Path, title: &str) -> String {
    let stem = title_to_slug(title);
    let project_relative = project_folder
        .strip_prefix(vault_path)
        .unwrap_or(project_folder);
    let project_relative_str = project_relative.to_string_lossy().replace('\\', "/");
    let mut parts: Vec<String> = Vec::new();
    if !project_relative_str.is_empty() && project_relative_str != "." {
        parts.push(project_relative_str);
    }
    parts.push(stem);
    parts.push("tasks".to_string());
    parts.join("/")
}

fn render_task_body(title: &str, project: Option<&str>) -> String {
    let project_line = match project {
        Some(p) if !p.is_empty() => format!("project: \"[[{p}]]\"\n"),
        _ => String::new(),
    };
    format!("---\ntype: task\n{project_line}---\n\n# {title}\n")
}

fn render_project_body(title: &str, task_folder: &str) -> String {
    format!(
        "---\ntype: project\ntask_folder: \"{task_folder}\"\nstatuses:\n  - \"Not started\"\n  - \"In progress\"\n  - Done\nterminal_statuses: [Done]\ndefault_view: board\n---\n\n# {title}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn vault() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn create_task_writes_file_and_seeds_type_doc() {
        let dir = vault();
        let vault_path = dir.path();

        let result = create_task_note(vault_path, "", "My First Task", None).unwrap();

        let task_path = vault_path.join("my-first-task.md");
        assert!(task_path.exists());
        assert_eq!(result.path, task_path.to_string_lossy());
        assert_eq!(
            result.warnings,
            vec!["seeded task.md type document at vault root"]
        );

        let task_doc = vault_path.join("task.md");
        assert!(task_doc.exists());
        let task_doc_body = std::fs::read_to_string(&task_doc).unwrap();
        assert!(task_doc_body.contains("type: Type"));
        assert!(task_doc_body.contains("# Task"));

        let content = std::fs::read_to_string(&task_path).unwrap();
        assert!(content.contains("type: task"));
        assert!(content.contains("# My First Task"));
    }

    #[test]
    fn create_task_second_call_does_not_reseed() {
        let dir = vault();
        let vault_path = dir.path();

        create_task_note(vault_path, "", "First", None).unwrap();
        let second = create_task_note(vault_path, "", "Second", None).unwrap();
        assert!(second.warnings.is_empty());
    }

    #[test]
    fn create_task_skips_seed_when_type_dir_doc_exists() {
        let dir = vault();
        let vault_path = dir.path();
        let type_dir = vault_path.join("type");
        std::fs::create_dir(&type_dir).unwrap();
        std::fs::write(type_dir.join("task.md"), "---\ntype: Type\n---\n# Task\n").unwrap();

        let result = create_task_note(vault_path, "", "Hello", None).unwrap();
        assert!(result.warnings.is_empty());
        assert!(!vault_path.join("task.md").exists());
    }

    #[test]
    fn create_task_resolves_filename_collisions_with_suffix() {
        let dir = vault();
        let vault_path = dir.path();

        let first = create_task_note(vault_path, "", "Repeat", None).unwrap();
        let second = create_task_note(vault_path, "", "Repeat", None).unwrap();
        let third = create_task_note(vault_path, "", "Repeat", None).unwrap();

        assert!(first.path.ends_with("repeat.md"));
        assert!(second.path.ends_with("repeat-2.md"));
        assert!(third.path.ends_with("repeat-3.md"));
    }

    #[test]
    fn create_task_writes_project_wikilink_when_provided() {
        let dir = vault();
        let vault_path = dir.path();

        let result = create_task_note(vault_path, "", "Sync work", Some("Q2 Launch")).unwrap();
        let content = std::fs::read_to_string(&result.path).unwrap();
        assert!(content.contains("project: \"[[Q2 Launch]]\""));
    }

    #[test]
    fn create_task_in_subfolder() {
        let dir = vault();
        let vault_path = dir.path();
        std::fs::create_dir(vault_path.join("Inbox")).unwrap();

        let result = create_task_note(vault_path, "Inbox", "Quick todo", None).unwrap();
        assert!(vault_path.join("Inbox").join("quick-todo.md").exists());
        assert!(result.path.contains("Inbox"));
    }

    #[test]
    fn create_project_writes_file_with_default_task_folder() {
        let dir = vault();
        let vault_path = dir.path();

        let result = create_project_note(vault_path, "", "Q2 Launch").unwrap();
        let project_path = vault_path.join("q2-launch.md");
        assert!(project_path.exists());
        assert_eq!(result.path, project_path.to_string_lossy());

        let content = std::fs::read_to_string(&project_path).unwrap();
        assert!(content.contains("type: project"));
        assert!(content.contains("task_folder:"));
        assert!(content.contains("q2-launch/tasks"));
        assert!(content.contains("terminal_statuses: [Done]"));
        assert!(content.contains("default_view: board"));
    }

    #[test]
    fn create_project_seeds_project_type_doc_on_first_call() {
        let dir = vault();
        let vault_path = dir.path();

        let result = create_project_note(vault_path, "", "Project A").unwrap();
        assert_eq!(
            result.warnings,
            vec!["seeded project.md type document at vault root"]
        );
        assert!(vault_path.join("project.md").exists());
    }

    #[test]
    fn whitespace_only_title_falls_back_to_untitled() {
        let dir = vault();
        let vault_path = dir.path();

        let result = create_task_note(vault_path, "", "   ", None).unwrap();
        assert!(result.path.ends_with("untitled.md"));
    }

    #[test]
    fn title_with_punctuation_gets_slugged() {
        let dir = vault();
        let vault_path = dir.path();

        let result = create_task_note(vault_path, "", "Q1: Plan & Ship!", None).unwrap();
        assert!(result.path.ends_with("q1-plan-ship.md"));
    }
}
