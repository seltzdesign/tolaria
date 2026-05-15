//! Push-side of the GitHub Projects v2 bridge.
//!
//! Mirrors local task notes back up to github.com on demand: tasks with no
//! `github_item_id` get created as draft issues, and field changes since the
//! last snapshot get applied as `UpdateProjectItemField` mutations. The
//! planning step is pure — it scans the task folder, parses frontmatter,
//! and produces a `PushPlan` of mutations. The Tauri command runs the plan
//! against the real GraphQL client; tests exercise the planner against
//! known frontmatter + a synthetic snapshot.
//!
//! ## Scope (P12)
//!
//! - Creates: any `type: task` note in the bound project's `task_folder`
//!   without a `github_item_id` becomes a draft issue. After the mutation
//!   succeeds, the new item id + sync timestamps are written back to the
//!   note so the next pull recognises it.
//! - Updates: for each mapped field, if the local value differs from the
//!   snapshot's last-seen remote value, an `UpdateProjectItemField`
//!   mutation runs.
//! - Out of scope: clearing fields, autosave-triggered push (P14 will
//!   address that once the editor/sync race is solved), and non-text /
//!   non-number / non-date / non-single-select field types.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_yaml::{Mapping, Value as YamlValue};

use super::queries::{FieldValueInput, ProjectField};

/// Inputs to one push planning pass. Symmetric with `sync::PullInput` so
/// the Tauri command can construct both from the same project binding.
pub struct PushInput {
    pub vault_path: PathBuf,
    pub task_folder_rel: String,
    pub project_node_id: String,
    /// GitHub field name used for the local `status` key. Treated
    /// separately from `field_mappings` to match how the binding stores it.
    pub status_field: Option<String>,
    pub field_mappings: Vec<(String, String)>,
    pub field_schema: Vec<ProjectField>,
    pub now_rfc3339: String,
}

/// A single `UpdateProjectItemField` mutation for an existing item. The
/// planner carries both the typed `FieldValueInput` (so the executor is
/// trivially dumb) and the value-as-string (so the snapshot can be
/// updated to the new state once the mutation succeeds).
#[derive(Debug, Clone, PartialEq)]
pub struct FieldUpdatePlan {
    pub local_file_path: String,
    pub item_id: String,
    pub field_name: String,
    pub field_id: String,
    pub value: FieldValueInput,
    pub remote_value_str: String,
}

/// Field update that depends on a draft-issue create in the same push
/// cycle. We don't have the item id at plan time; the executor fills it in
/// after `AddDraftIssue` resolves.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingFieldUpdate {
    pub field_name: String,
    pub field_id: String,
    pub value: FieldValueInput,
    pub remote_value_str: String,
}

/// Plan for creating one new draft issue + applying all of its mapped
/// field values. The local file path is carried through so we can write
/// the new item id back to the note's frontmatter on success.
#[derive(Debug, Clone, PartialEq)]
pub struct DraftCreatePlan {
    pub local_file_path: String,
    pub title: String,
    pub body: Option<String>,
    pub follow_up_fields: Vec<PendingFieldUpdate>,
}

/// The full set of mutations the executor needs to run, plus any
/// non-fatal warnings the planner produced (e.g. unmapped field types).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PushPlan {
    pub creates: Vec<DraftCreatePlan>,
    pub updates: Vec<FieldUpdatePlan>,
    pub warnings: Vec<String>,
}

/// Walk the bound project's task folder and produce a `PushPlan`. Files
/// that fail to parse, lack `type: task`, or are bound to a different
/// project are skipped silently (warnings are reserved for actionable
/// cases like unmapped field types so the user can see what to fix).
pub fn plan_push(
    input: &PushInput,
    snapshot_items: &BTreeMap<String, super::snapshot::SnapshotItem>,
) -> Result<PushPlan, String> {
    let task_folder_abs = input.vault_path.join(&input.task_folder_rel);
    let entries = match fs::read_dir(&task_folder_abs) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PushPlan::default());
        }
        Err(e) => {
            return Err(format!(
                "Failed to list task folder `{}`: {e}",
                task_folder_abs.display()
            ));
        }
    };

    let schema_lookup: BTreeMap<&str, &ProjectField> = input
        .field_schema
        .iter()
        .map(|f| (f.name.as_str(), f))
        .collect();
    let mut plan = PushPlan::default();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok((frontmatter, body)) = split_frontmatter(&content) else {
            continue;
        };
        let task_type = frontmatter_str(&frontmatter, "type");
        if task_type.as_deref() != Some("task") {
            continue;
        }

        let rel_path = relative_to_vault(&input.vault_path, &path);
        let title = frontmatter_str(&frontmatter, "title").unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let item_id = frontmatter_str(&frontmatter, "github_item_id");
        let existing_project = frontmatter_str(&frontmatter, "github_project_node_id");

        // Tasks bound to a different project must not be touched.
        if let Some(existing_project) = &existing_project {
            if existing_project != &input.project_node_id {
                continue;
            }
        }

        let local_fields = collect_local_field_values(
            &frontmatter,
            input.status_field.as_deref(),
            &input.field_mappings,
        );

        match item_id {
            None => {
                let mut follow_up = Vec::new();
                for (field_name, raw_value) in &local_fields {
                    match build_field_value(&schema_lookup, field_name, raw_value) {
                        Ok(Some((field_id, typed))) => follow_up.push(PendingFieldUpdate {
                            field_name: field_name.clone(),
                            field_id,
                            value: typed,
                            remote_value_str: raw_value.clone(),
                        }),
                        Ok(None) => {}
                        Err(warning) => plan.warnings.push(warning),
                    }
                }
                plan.creates.push(DraftCreatePlan {
                    local_file_path: rel_path,
                    title,
                    body: body_for_create(&body),
                    follow_up_fields: follow_up,
                });
            }
            Some(item_id) => {
                let snapshot_for_item = snapshot_items.get(&item_id);
                for (field_name, raw_value) in &local_fields {
                    let last_seen = snapshot_for_item
                        .and_then(|s| s.field_values.get(field_name))
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    if last_seen == raw_value {
                        continue;
                    }
                    match build_field_value(&schema_lookup, field_name, raw_value) {
                        Ok(Some((field_id, typed))) => plan.updates.push(FieldUpdatePlan {
                            local_file_path: rel_path.clone(),
                            item_id: item_id.clone(),
                            field_name: field_name.clone(),
                            field_id,
                            value: typed,
                            remote_value_str: raw_value.clone(),
                        }),
                        Ok(None) => {}
                        Err(warning) => plan.warnings.push(warning),
                    }
                }
            }
        }
    }

    plan.creates
        .sort_by(|a, b| a.local_file_path.cmp(&b.local_file_path));
    plan.updates.sort_by(|a, b| {
        a.local_file_path
            .cmp(&b.local_file_path)
            .then_with(|| a.field_name.cmp(&b.field_name))
    });
    Ok(plan)
}

/// Collect the current local value for every field that has a mapping,
/// keyed by **GitHub field name** so the result lines up with the
/// snapshot's `field_values`. Values are stringified — numbers/dates
/// emitted by the YAML parser are normalised back to the same string
/// form the snapshot stored.
fn collect_local_field_values(
    frontmatter: &Mapping,
    status_field: Option<&str>,
    field_mappings: &[(String, String)],
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    if let Some(status_field) = status_field {
        if let Some(value) = frontmatter_value_string(frontmatter, "status") {
            out.push((status_field.to_string(), value));
        }
    }
    for (local_key, github_name) in field_mappings {
        if local_key == "status" {
            continue;
        }
        if let Some(value) = frontmatter_value_string(frontmatter, local_key) {
            out.push((github_name.clone(), value));
        }
    }
    out
}

/// Build the typed `FieldValueInput` for one (field, value) pair. Returns
/// `Ok(None)` when the value is empty (nothing to push) and an `Err`
/// string when the value couldn't be coerced (e.g. SingleSelect with no
/// matching option) — those become user-visible warnings in the plan.
fn build_field_value(
    schema_lookup: &BTreeMap<&str, &ProjectField>,
    field_name: &str,
    raw_value: &str,
) -> Result<Option<(String, FieldValueInput)>, String> {
    let trimmed = raw_value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let field = schema_lookup.get(field_name).ok_or_else(|| {
        format!("Skipped `{field_name}`: not present on the bound GitHub project.")
    })?;
    let input = match field.data_type.as_str() {
        "TEXT" => FieldValueInput::text(trimmed),
        "NUMBER" => {
            let parsed: f64 = trimmed
                .parse()
                .map_err(|_| format!("Skipped `{field_name}`: `{trimmed}` is not a number."))?;
            FieldValueInput::number(parsed)
        }
        "DATE" => FieldValueInput::date(trimmed),
        "SINGLE_SELECT" => {
            let option = field
                .options
                .iter()
                .find(|opt| opt.name.eq_ignore_ascii_case(trimmed))
                .ok_or_else(|| {
                    format!("Skipped `{field_name}`: `{trimmed}` is not an option on this field.")
                })?;
            FieldValueInput::single_select(option.id.clone())
        }
        other => {
            return Err(format!(
                "Skipped `{field_name}`: field type `{other}` is not yet supported for push."
            ));
        }
    };
    Ok(Some((field.id.clone(), input)))
}

/// After a draft create succeeds, stamp the new GitHub identifiers into
/// the local task file's frontmatter so the next sync recognises it as
/// already-bound. The body is left untouched. This mirrors the binding
/// module's "modify-frontmatter-in-place" pattern but on a task note
/// rather than a project note.
pub fn write_back_create_metadata(
    note_path: &Path,
    project_node_id: &str,
    item_id: &str,
    now_rfc3339: &str,
) -> Result<(), String> {
    let content = fs::read_to_string(note_path)
        .map_err(|e| format!("Failed to read task `{}`: {e}", note_path.display()))?;
    let (mut mapping, body) = split_frontmatter(&content)?;
    mapping.insert(
        YamlValue::String("github_project_node_id".into()),
        YamlValue::String(project_node_id.to_string()),
    );
    mapping.insert(
        YamlValue::String("github_item_id".into()),
        YamlValue::String(item_id.to_string()),
    );
    mapping.insert(
        YamlValue::String("github_content_type".into()),
        YamlValue::String("DraftIssue".into()),
    );
    mapping.insert(
        YamlValue::String("github_last_synced".into()),
        YamlValue::String(now_rfc3339.to_string()),
    );
    let yaml = serde_yaml::to_string(&YamlValue::Mapping(mapping))
        .map_err(|e| format!("Failed to serialize task frontmatter: {e}"))?;
    let yaml_trimmed = yaml.trim_end_matches('\n');
    let body_block = if body.is_empty() {
        String::new()
    } else {
        format!("\n{}", body.trim_start_matches('\n'))
    };
    fs::write(note_path, format!("---\n{yaml_trimmed}\n---{body_block}"))
        .map_err(|e| format!("Failed to write task `{}`: {e}", note_path.display()))
}

fn relative_to_vault(vault: &Path, abs: &Path) -> String {
    abs.strip_prefix(vault)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| abs.to_string_lossy().into_owned())
}

fn body_for_create(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn split_frontmatter(content: &str) -> Result<(Mapping, String), String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok((Mapping::new(), content.to_string()));
    }
    let opener = content.find("---").expect("just checked the prefix");
    let after_open = &content[opener + 3..];
    let after_open = after_open.trim_start_matches('\r').trim_start_matches('\n');
    let close = after_open
        .find("\n---")
        .ok_or_else(|| "Task frontmatter is unterminated.".to_string())?;
    let fm_text = &after_open[..close];
    let body_after = after_open[close + 4..]
        .trim_start_matches('\r')
        .trim_start_matches('\n')
        .to_string();
    let mapping: Mapping = if fm_text.trim().is_empty() {
        Mapping::new()
    } else {
        serde_yaml::from_str(fm_text)
            .map_err(|e| format!("Failed to parse task frontmatter: {e}"))?
    };
    Ok((mapping, body_after))
}

fn frontmatter_str(mapping: &Mapping, key: &str) -> Option<String> {
    mapping
        .get(YamlValue::String(key.to_string()))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

/// Pull any scalar frontmatter value back into a string. Numbers and
/// booleans get the same string form the snapshot stores so a value that
/// pulled down as `estimate: 5` (number) compares equal to the
/// snapshot's `"5"` and doesn't trigger a redundant push.
fn frontmatter_value_string(mapping: &Mapping, key: &str) -> Option<String> {
    let value = mapping.get(YamlValue::String(key.to_string()))?;
    match value {
        YamlValue::String(s) => Some(s.clone()),
        YamlValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i.to_string())
            } else {
                n.as_f64().map(|f| {
                    if f.fract() == 0.0 {
                        format!("{}", f as i64)
                    } else {
                        format!("{f}")
                    }
                })
            }
        }
        YamlValue::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::projects::queries::ProjectFieldOption;
    use crate::github::projects::snapshot::SnapshotItem;
    use tempfile::TempDir;

    fn schema_for_tests() -> Vec<ProjectField> {
        vec![
            ProjectField {
                id: "FID_status".into(),
                name: "Status".into(),
                data_type: "SINGLE_SELECT".into(),
                options: vec![
                    ProjectFieldOption {
                        id: "OPT_backlog".into(),
                        name: "Backlog".into(),
                    },
                    ProjectFieldOption {
                        id: "OPT_done".into(),
                        name: "Done".into(),
                    },
                ],
            },
            ProjectField {
                id: "FID_priority".into(),
                name: "Priority".into(),
                data_type: "TEXT".into(),
                options: vec![],
            },
            ProjectField {
                id: "FID_estimate".into(),
                name: "Estimate".into(),
                data_type: "NUMBER".into(),
                options: vec![],
            },
            ProjectField {
                id: "FID_due".into(),
                name: "Target date".into(),
                data_type: "DATE".into(),
                options: vec![],
            },
        ]
    }

    fn make_input(vault: &Path) -> PushInput {
        PushInput {
            vault_path: vault.to_path_buf(),
            task_folder_rel: "tasks".into(),
            project_node_id: "PVT_demo".into(),
            status_field: Some("Status".into()),
            field_mappings: vec![
                ("priority".into(), "Priority".into()),
                ("estimate".into(), "Estimate".into()),
                ("due".into(), "Target date".into()),
            ],
            field_schema: schema_for_tests(),
            now_rfc3339: "2026-05-15T13:00:00Z".into(),
        }
    }

    fn write_task(vault: &Path, name: &str, content: &str) {
        let dir = vault.join("tasks");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn new_local_task_is_planned_as_a_draft_create() {
        let vault = TempDir::new().unwrap();
        write_task(
            vault.path(),
            "ship.md",
            "---\ntype: task\ntitle: Ship the bridge\nstatus: Backlog\npriority: P0\nestimate: 5\n---\n# Ship the bridge\n\nGo go go\n",
        );
        let plan = plan_push(&make_input(vault.path()), &BTreeMap::new()).unwrap();
        assert_eq!(plan.creates.len(), 1);
        let create = &plan.creates[0];
        assert_eq!(create.title, "Ship the bridge");
        assert_eq!(
            create.body.as_deref(),
            Some("# Ship the bridge\n\nGo go go")
        );
        let names: Vec<_> = create
            .follow_up_fields
            .iter()
            .map(|f| &f.field_name)
            .collect();
        assert!(names.contains(&&"Status".to_string()));
        assert!(names.contains(&&"Priority".to_string()));
        assert!(names.contains(&&"Estimate".to_string()));
    }

    #[test]
    fn existing_task_with_changed_field_is_planned_as_an_update() {
        let vault = TempDir::new().unwrap();
        write_task(
            vault.path(),
            "ship.md",
            "---\ntype: task\ntitle: Ship the bridge\ngithub_project_node_id: PVT_demo\ngithub_item_id: PVTI_x\nstatus: Done\npriority: P0\n---\n# Ship the bridge\n",
        );
        let mut snapshot_items = BTreeMap::new();
        snapshot_items.insert(
            "PVTI_x".into(),
            SnapshotItem {
                item_id: "PVTI_x".into(),
                content_type: "DraftIssue".into(),
                title: "Ship the bridge".into(),
                body: None,
                url: None,
                number: None,
                repository: None,
                field_values: {
                    let mut map = BTreeMap::new();
                    map.insert("Status".into(), "Backlog".into());
                    map.insert("Priority".into(), "P0".into());
                    map
                },
                local_file_path: "tasks/ship.md".into(),
            },
        );
        let plan = plan_push(&make_input(vault.path()), &snapshot_items).unwrap();
        assert_eq!(plan.creates.len(), 0);
        assert_eq!(plan.updates.len(), 1);
        assert_eq!(plan.updates[0].field_name, "Status");
        assert_eq!(plan.updates[0].remote_value_str, "Done");
    }

    #[test]
    fn matching_field_values_produce_no_update() {
        let vault = TempDir::new().unwrap();
        write_task(
            vault.path(),
            "ship.md",
            "---\ntype: task\ntitle: Ship\ngithub_project_node_id: PVT_demo\ngithub_item_id: PVTI_x\nstatus: Backlog\nestimate: 7\n---\n# Ship\n",
        );
        let mut snapshot_items = BTreeMap::new();
        snapshot_items.insert(
            "PVTI_x".into(),
            SnapshotItem {
                item_id: "PVTI_x".into(),
                content_type: "DraftIssue".into(),
                title: "Ship".into(),
                body: None,
                url: None,
                number: None,
                repository: None,
                field_values: {
                    let mut map = BTreeMap::new();
                    map.insert("Status".into(), "Backlog".into());
                    map.insert("Estimate".into(), "7".into());
                    map
                },
                local_file_path: "tasks/ship.md".into(),
            },
        );
        let plan = plan_push(&make_input(vault.path()), &snapshot_items).unwrap();
        assert!(plan.updates.is_empty());
    }

    #[test]
    fn task_bound_to_different_project_is_skipped() {
        let vault = TempDir::new().unwrap();
        write_task(
            vault.path(),
            "elsewhere.md",
            "---\ntype: task\ntitle: Elsewhere\ngithub_project_node_id: PVT_other\ngithub_item_id: PVTI_y\nstatus: Backlog\n---\n",
        );
        let plan = plan_push(&make_input(vault.path()), &BTreeMap::new()).unwrap();
        assert!(plan.creates.is_empty());
        assert!(plan.updates.is_empty());
    }

    #[test]
    fn unsupported_field_type_emits_a_warning_not_an_error() {
        let vault = TempDir::new().unwrap();
        write_task(
            vault.path(),
            "ship.md",
            "---\ntype: task\ntitle: Ship\nstatus: Backlog\nlabels: bug\n---\n",
        );
        let mut input = make_input(vault.path());
        // Map a local key to a GH field whose data_type we don't support yet.
        input
            .field_mappings
            .push(("labels".into(), "Labels".into()));
        input.field_schema.push(ProjectField {
            id: "FID_labels".into(),
            name: "Labels".into(),
            data_type: "LABELS".into(),
            options: vec![],
        });
        let plan = plan_push(&input, &BTreeMap::new()).unwrap();
        assert_eq!(plan.creates.len(), 1);
        let warning_joined = plan.warnings.join(" | ");
        assert!(
            warning_joined.contains("labels") || warning_joined.contains("Labels"),
            "expected a labels warning, got: {warning_joined}"
        );
    }

    #[test]
    fn single_select_value_must_match_an_option_name() {
        let vault = TempDir::new().unwrap();
        write_task(
            vault.path(),
            "ship.md",
            "---\ntype: task\ntitle: Ship\nstatus: NotAReal Option\n---\n",
        );
        let plan = plan_push(&make_input(vault.path()), &BTreeMap::new()).unwrap();
        assert_eq!(plan.creates.len(), 1);
        assert!(plan.creates[0]
            .follow_up_fields
            .iter()
            .all(|f| f.field_name != "Status"));
        assert!(plan
            .warnings
            .iter()
            .any(|w| w.to_lowercase().contains("status")));
    }

    #[test]
    fn frontmatter_numbers_stringify_consistently_with_snapshot() {
        assert_eq!(
            frontmatter_value_string(&serde_yaml::from_str::<Mapping>("n: 5\n").unwrap(), "n"),
            Some("5".to_string())
        );
        assert_eq!(
            frontmatter_value_string(&serde_yaml::from_str::<Mapping>("n: 5.5\n").unwrap(), "n"),
            Some("5.5".to_string())
        );
        assert_eq!(
            frontmatter_value_string(&serde_yaml::from_str::<Mapping>("s: hello\n").unwrap(), "s"),
            Some("hello".to_string())
        );
    }

    #[test]
    fn missing_task_folder_returns_empty_plan_not_error() {
        let vault = TempDir::new().unwrap();
        let plan = plan_push(&make_input(vault.path()), &BTreeMap::new()).unwrap();
        assert!(plan.creates.is_empty());
        assert!(plan.updates.is_empty());
    }

    #[test]
    fn write_back_create_metadata_stamps_ids_without_dropping_body() {
        let vault = TempDir::new().unwrap();
        let task_path = vault.path().join("ship.md");
        fs::write(
            &task_path,
            "---\ntype: task\ntitle: Ship\nstatus: Backlog\n---\n# Ship\n\nBody intact\n",
        )
        .unwrap();
        write_back_create_metadata(&task_path, "PVT_demo", "PVTI_new", "2026-05-15T13:00:00Z")
            .unwrap();
        let updated = fs::read_to_string(&task_path).unwrap();
        assert!(updated.contains("github_project_node_id: PVT_demo"));
        assert!(updated.contains("github_item_id: PVTI_new"));
        assert!(updated.contains("github_content_type: DraftIssue"));
        assert!(updated.contains("github_last_synced: 2026-05-15T13:00:00Z"));
        assert!(updated.contains("# Ship"));
        assert!(updated.contains("Body intact"));
    }
}
