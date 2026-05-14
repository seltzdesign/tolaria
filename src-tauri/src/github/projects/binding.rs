//! Write/clear the GitHub Projects v2 binding fields on a project note.
//!
//! The binding lives in the project note's YAML frontmatter (see ADR 0115).
//! Binding is an explicit, atomic, user-initiated operation, so we round-trip
//! the frontmatter through `serde_yaml::Mapping` rather than poking
//! individual lines — that's the only way to write a nested `field_mappings`
//! map cleanly. Everyday per-field edits (status, due, etc.) still go
//! through the line-based update path; only Bind/Unbind take this code path.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value as YamlValue};

const BRIDGE_KEYS: &[&str] = &[
    "github_project_url",
    "github_project_node_id",
    "sync_enabled",
    "sync_interval_minutes",
    "link_to_issues",
    "github_issue_repo",
    "status_field",
    "field_mappings",
];

const DEFAULT_SYNC_INTERVAL_MINUTES: u32 = 5;

/// User-supplied binding payload from the Bind modal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubBindingInput {
    pub project_url: String,
    pub project_node_id: String,
    /// Optional override; default `DEFAULT_SYNC_INTERVAL_MINUTES`.
    pub sync_interval_minutes: Option<u32>,
    /// Optional — only used when `link_to_issues == true`.
    pub link_to_issues: Option<bool>,
    pub github_issue_repo: Option<String>,
    /// GH field name (NOT ID) mapped to the local `status` field.
    pub status_field: Option<String>,
    /// Mapping of local field key → GH field name. Use empty Vec to skip.
    pub field_mappings: Vec<FieldMappingEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMappingEntry {
    pub local: String,
    pub github: String,
}

pub fn write_binding(note_path: &Path, input: &GithubBindingInput) -> Result<(), String> {
    apply_binding_in_place(note_path, |mapping| {
        apply_binding_to_mapping(mapping, input);
    })
}

pub fn clear_binding(note_path: &Path) -> Result<(), String> {
    apply_binding_in_place(note_path, |mapping| {
        for key in BRIDGE_KEYS {
            mapping.remove(YamlValue::String((*key).to_string()));
        }
    })
}

fn apply_binding_in_place<F: FnOnce(&mut Mapping)>(
    note_path: &Path,
    mutator: F,
) -> Result<(), String> {
    let content = fs::read_to_string(note_path)
        .map_err(|e| format!("Failed to read project note `{}`: {e}", note_path.display()))?;
    let (mut frontmatter, body) = split_frontmatter(&content)?;
    mutator(&mut frontmatter);
    let rebuilt = rebuild_with_frontmatter(&frontmatter, body)?;
    fs::write(note_path, rebuilt).map_err(|e| {
        format!(
            "Failed to write project note `{}`: {e}",
            note_path.display()
        )
    })
}

fn apply_binding_to_mapping(mapping: &mut Mapping, input: &GithubBindingInput) {
    set_string(mapping, "github_project_url", &input.project_url);
    set_string(mapping, "github_project_node_id", &input.project_node_id);
    set_bool(mapping, "sync_enabled", true);
    set_u32(
        mapping,
        "sync_interval_minutes",
        input
            .sync_interval_minutes
            .unwrap_or(DEFAULT_SYNC_INTERVAL_MINUTES),
    );
    match input.link_to_issues {
        Some(true) => {
            set_bool(mapping, "link_to_issues", true);
            if let Some(repo) = input.github_issue_repo.as_deref() {
                set_string(mapping, "github_issue_repo", repo);
            }
        }
        _ => {
            mapping.remove(YamlValue::String("link_to_issues".to_string()));
            mapping.remove(YamlValue::String("github_issue_repo".to_string()));
        }
    }
    if let Some(status_field) = input.status_field.as_deref() {
        if !status_field.is_empty() {
            set_string(mapping, "status_field", status_field);
        }
    }
    if input.field_mappings.is_empty() {
        mapping.remove(YamlValue::String("field_mappings".to_string()));
    } else {
        let mut field_map = Mapping::new();
        for entry in &input.field_mappings {
            if entry.local.is_empty() || entry.github.is_empty() {
                continue;
            }
            field_map.insert(
                YamlValue::String(entry.local.clone()),
                YamlValue::String(entry.github.clone()),
            );
        }
        mapping.insert(
            YamlValue::String("field_mappings".to_string()),
            YamlValue::Mapping(field_map),
        );
    }
}

fn set_string(mapping: &mut Mapping, key: &str, value: &str) {
    mapping.insert(
        YamlValue::String(key.to_string()),
        YamlValue::String(value.to_string()),
    );
}

fn set_bool(mapping: &mut Mapping, key: &str, value: bool) {
    mapping.insert(YamlValue::String(key.to_string()), YamlValue::Bool(value));
}

fn set_u32(mapping: &mut Mapping, key: &str, value: u32) {
    mapping.insert(
        YamlValue::String(key.to_string()),
        YamlValue::Number(serde_yaml::Number::from(value)),
    );
}

fn split_frontmatter(content: &str) -> Result<(Mapping, &str), String> {
    let trimmed = content.trim_start();
    let starts_with_delim = trimmed.starts_with("---");
    if !starts_with_delim {
        return Ok((Mapping::new(), content));
    }
    let after_open = &content[content.find("---").unwrap() + 3..];
    let after_open = after_open.trim_start_matches('\r').trim_start_matches('\n');
    let close = after_open
        .find("\n---")
        .ok_or_else(|| "Project note frontmatter is unterminated.".to_string())?;
    let fm_text = &after_open[..close];
    let body_start = close + 4;
    let body = after_open[body_start..]
        .trim_start_matches('\r')
        .trim_start_matches('\n');
    let mapping: Mapping = if fm_text.trim().is_empty() {
        Mapping::new()
    } else {
        serde_yaml::from_str(fm_text)
            .map_err(|e| format!("Failed to parse project note frontmatter: {e}"))?
    };
    Ok((mapping, body))
}

fn rebuild_with_frontmatter(mapping: &Mapping, body: &str) -> Result<String, String> {
    let yaml = serde_yaml::to_string(&YamlValue::Mapping(mapping.clone()))
        .map_err(|e| format!("Failed to serialize frontmatter: {e}"))?;
    let yaml_trimmed = yaml.trim_end_matches('\n');
    let body_normalized = if body.is_empty() {
        String::new()
    } else {
        format!("\n{}", body.trim_start_matches('\n'))
    };
    Ok(format!("---\n{}\n---{}", yaml_trimmed, body_normalized))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_input() -> GithubBindingInput {
        GithubBindingInput {
            project_url: "https://github.com/users/x/projects/7".into(),
            project_node_id: "PVT_kwHO_abc".into(),
            sync_interval_minutes: Some(7),
            link_to_issues: None,
            github_issue_repo: None,
            status_field: Some("Status".into()),
            field_mappings: vec![
                FieldMappingEntry {
                    local: "priority".into(),
                    github: "Priority".into(),
                },
                FieldMappingEntry {
                    local: "due".into(),
                    github: "End date".into(),
                },
            ],
        }
    }

    fn write_note(dir: &TempDir, content: &str) -> std::path::PathBuf {
        let path = dir.path().join("project.md");
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn writes_bridge_fields_into_an_existing_frontmatter_block() {
        let dir = TempDir::new().unwrap();
        let path = write_note(
            &dir,
            "---\ntype: project\ntask_folder: tasks/q2\n---\n\n# Q2 Launch\n",
        );
        write_binding(&path, &sample_input()).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.contains("type: project"));
        assert!(updated.contains("task_folder: tasks/q2"));
        assert!(updated.contains("github_project_url: https://github.com/users/x/projects/7"));
        assert!(updated.contains("github_project_node_id: PVT_kwHO_abc"));
        assert!(updated.contains("sync_enabled: true"));
        assert!(updated.contains("sync_interval_minutes: 7"));
        assert!(updated.contains("status_field: Status"));
        assert!(updated.contains("field_mappings:"));
        assert!(updated.contains("priority: Priority"));
        assert!(updated.contains("# Q2 Launch"));
    }

    #[test]
    fn omits_field_mappings_block_when_input_is_empty() {
        let dir = TempDir::new().unwrap();
        let path = write_note(&dir, "---\ntype: project\n---\n\nbody\n");
        let mut input = sample_input();
        input.field_mappings.clear();
        write_binding(&path, &input).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        assert!(!updated.contains("field_mappings"));
    }

    #[test]
    fn link_to_issues_writes_repo_only_when_enabled() {
        let dir = TempDir::new().unwrap();
        let path = write_note(&dir, "---\ntype: project\n---\nbody\n");
        let mut input = sample_input();
        input.link_to_issues = Some(true);
        input.github_issue_repo = Some("seltzdesign/foo".into());
        write_binding(&path, &input).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.contains("link_to_issues: true"));
        assert!(updated.contains("github_issue_repo: seltzdesign/foo"));
    }

    #[test]
    fn link_to_issues_disabled_strips_existing_repo_value() {
        let dir = TempDir::new().unwrap();
        let path = write_note(
            &dir,
            "---\ntype: project\nlink_to_issues: true\ngithub_issue_repo: old/repo\n---\nbody\n",
        );
        let input = sample_input();
        write_binding(&path, &input).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        assert!(!updated.contains("link_to_issues"));
        assert!(!updated.contains("github_issue_repo"));
    }

    #[test]
    fn rebinding_replaces_previous_bridge_values() {
        let dir = TempDir::new().unwrap();
        let path = write_note(
            &dir,
            "---\ntype: project\ngithub_project_url: https://github.com/users/x/projects/1\ngithub_project_node_id: PVT_old\n---\nbody\n",
        );
        write_binding(&path, &sample_input()).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        assert!(!updated.contains("PVT_old"));
        assert!(updated.contains("PVT_kwHO_abc"));
    }

    #[test]
    fn clear_binding_removes_only_bridge_fields() {
        let dir = TempDir::new().unwrap();
        let path = write_note(&dir, "---\ntype: project\n---\nbody\n");
        write_binding(&path, &sample_input()).unwrap();
        clear_binding(&path).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        for key in BRIDGE_KEYS {
            assert!(
                !updated.contains(&format!("{key}:")),
                "{key} should be gone"
            );
        }
        assert!(updated.contains("type: project"));
        assert!(updated.contains("body"));
    }

    #[test]
    fn writes_frontmatter_when_note_has_none() {
        let dir = TempDir::new().unwrap();
        let path = write_note(&dir, "# Plain project note\n");
        write_binding(&path, &sample_input()).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.starts_with("---\n"));
        assert!(updated.contains("github_project_url"));
        assert!(updated.contains("# Plain project note"));
    }

    #[test]
    fn rejects_a_note_with_unterminated_frontmatter() {
        let dir = TempDir::new().unwrap();
        let path = write_note(&dir, "---\ntype: project\nstill no close");
        let err = write_binding(&path, &sample_input()).unwrap_err();
        assert!(err.contains("unterminated"));
    }
}
