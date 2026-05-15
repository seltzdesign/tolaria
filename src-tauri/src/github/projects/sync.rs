//! Pull-side of the GitHub Projects v2 bridge.
//!
//! Takes a snapshot of "what we last saw on github.com" (see [`super::snapshot`])
//! and a freshly fetched list of remote items, computes the create / update /
//! delete actions, and applies them to the bound project's task folder inside
//! the vault. Push-side (debounced field updates from local edits) lands in
//! P12 — this module deliberately only writes locally.
//!
//! ## Body handling
//!
//! On *create*, we copy the remote draft body into the local task note so the
//! task starts with the same description GitHub has. On *update*, we deliberately
//! leave the body alone and only refresh frontmatter. The body is where local
//! work happens (subtasks, links, notes); overwriting it on every pull would be
//! hostile. The GitHub-side description is editable on github.com if you want
//! to change it there, and an explicit re-sync of bodies is a P13-or-later
//! concern that will need conflict UI anyway.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_yaml::{Mapping, Value as YamlValue};

use super::queries::{FieldValue, ProjectItem, ProjectItemContent};
use super::snapshot::{self, ProjectSnapshot, SnapshotItem};

/// Inputs to one pull cycle. Constructed by the Tauri command from the
/// project note's frontmatter + a fresh `ListProjectItems` page.
pub struct PullInput {
    pub vault_path: PathBuf,
    /// Vault-relative folder where this project's task notes live.
    /// Read from the project note's `task_folder` frontmatter key.
    pub task_folder_rel: String,
    pub project_node_id: String,
    /// Filename stem of the project note, used to build the
    /// `project: "[[stem]]"` wikilink on task creation.
    pub project_note_stem: String,
    /// Optional GitHub field name mapped to the local `status` frontmatter
    /// key. Treated separately from `field_mappings` so the Bind modal can
    /// surface it as a first-class concept.
    pub status_field: Option<String>,
    /// Local frontmatter key → GitHub field name. Used to translate remote
    /// field values into local frontmatter keys on create/update.
    pub field_mappings: Vec<(String, String)>,
    pub items: Vec<ProjectItem>,
    /// Current wall-clock time as an RFC 3339 string. Plumbed in so tests
    /// can fix a deterministic timestamp in the snapshot/output frontmatter.
    pub now_rfc3339: String,
    /// Base directory the snapshot store should write into. The Tauri
    /// command resolves this from [`snapshot::default_base`]; tests pass a
    /// tempdir so they can run in parallel without env-var clashes.
    pub cache_base: PathBuf,
}

/// Counts plus a short error list. Errors don't abort the cycle: we apply
/// every action we can and surface the rest so the user sees partial
/// success instead of a silent half-finished sync.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PullSummary {
    pub created: u32,
    pub updated: u32,
    pub deleted: u32,
    pub unchanged: u32,
    pub errors: Vec<String>,
}

/// Apply one pull cycle. Returns the summary and persists the refreshed
/// snapshot on the way out (only when at least one item action ran without
/// erroring — a snapshot that erases unsynced items would lose track of
/// what we believe github.com has).
pub fn pull(input: &PullInput) -> Result<PullSummary, String> {
    let task_folder_abs = input.vault_path.join(&input.task_folder_rel);
    fs::create_dir_all(&task_folder_abs).map_err(|e| {
        format!(
            "Failed to create task folder `{}`: {e}",
            task_folder_abs.display()
        )
    })?;

    let prior = snapshot::load(&input.cache_base, &input.project_node_id);
    let mut next_items: BTreeMap<String, SnapshotItem> = BTreeMap::new();
    let mut summary = PullSummary::default();

    let remote_ids: HashSet<String> = input.items.iter().map(|i| i.id.clone()).collect();
    for item in &input.items {
        let Some(summary_view) = build_remote_summary(item) else {
            continue;
        };
        apply_remote_item(input, &task_folder_abs, &prior, &summary_view, &mut summary)
            .map(|snap_item| {
                next_items.insert(item.id.clone(), snap_item);
            })
            .unwrap_or_else(|err| summary.errors.push(err));
    }

    for (item_id, snap_item) in &prior.items {
        if remote_ids.contains(item_id) {
            continue;
        }
        match delete_local_file(&input.vault_path, &snap_item.local_file_path) {
            Ok(()) => summary.deleted += 1,
            Err(err) => summary.errors.push(err),
        }
    }

    let next_snapshot = ProjectSnapshot {
        version: prior.version,
        project_node_id: input.project_node_id.clone(),
        synced_at: input.now_rfc3339.clone(),
        items: next_items,
    };
    snapshot::save(&input.cache_base, &next_snapshot)?;
    Ok(summary)
}

/// Trimmed-down projection of one `ProjectItem` we care about: drops the GH
/// field-value enum variants down to a `name → value-string` map keyed by
/// GH field name (matches the bindings layer).
struct RemoteSummary {
    item_id: String,
    title: String,
    content_type: String,
    body: Option<String>,
    url: Option<String>,
    number: Option<i32>,
    repository: Option<String>,
    field_values: BTreeMap<String, String>,
}

fn build_remote_summary(item: &ProjectItem) -> Option<RemoteSummary> {
    let content = item.content.as_ref()?;
    let (title, content_type, body, url, number, repository) = match content {
        ProjectItemContent::DraftIssue { title, body } => (
            title.clone(),
            "DraftIssue".to_string(),
            body.clone(),
            None,
            None,
            None,
        ),
        ProjectItemContent::Issue {
            number,
            title,
            url,
            repository,
        } => (
            title.clone(),
            "Issue".to_string(),
            None,
            Some(url.clone()),
            Some(*number),
            Some(repository.name_with_owner.clone()),
        ),
        ProjectItemContent::PullRequest {
            number,
            title,
            url,
            repository,
        } => (
            title.clone(),
            "PullRequest".to_string(),
            None,
            Some(url.clone()),
            Some(*number),
            Some(repository.name_with_owner.clone()),
        ),
    };
    Some(RemoteSummary {
        item_id: item.id.clone(),
        title,
        content_type,
        body,
        url,
        number,
        repository,
        field_values: project_field_values(&item.field_values.nodes),
    })
}

fn project_field_values(values: &[FieldValue]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for value in values {
        match value {
            FieldValue::ProjectV2ItemFieldTextValue { text, field } => {
                if let Some(text) = text.clone() {
                    out.insert(field.name.clone(), text);
                }
            }
            FieldValue::ProjectV2ItemFieldNumberValue { number, field } => {
                if let Some(n) = number {
                    out.insert(field.name.clone(), format_number(*n));
                }
            }
            FieldValue::ProjectV2ItemFieldDateValue { date, field } => {
                if let Some(d) = date.clone() {
                    out.insert(field.name.clone(), d);
                }
            }
            FieldValue::ProjectV2ItemFieldSingleSelectValue { name, field, .. } => {
                if let Some(n) = name.clone() {
                    out.insert(field.name.clone(), n);
                }
            }
            FieldValue::Unknown => {}
        }
    }
    out
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// One remote item → one local action. Returns the snapshot entry that
/// should be carried forward; errors surface to the caller via a Result so
/// they accumulate without poisoning the rest of the cycle.
fn apply_remote_item(
    input: &PullInput,
    task_folder_abs: &Path,
    prior: &ProjectSnapshot,
    remote: &RemoteSummary,
    summary: &mut PullSummary,
) -> Result<SnapshotItem, String> {
    let frontmatter = build_frontmatter(input, remote);

    if let Some(prev) = prior.items.get(&remote.item_id) {
        let prev_view = SnapshotItem {
            local_file_path: prev.local_file_path.clone(),
            ..prev.clone()
        };
        let next_view = snapshot_view_from(remote, &prev.local_file_path);
        if prev_view == next_view {
            summary.unchanged += 1;
            return Ok(next_view);
        }
        update_existing_task_note(&input.vault_path, &prev.local_file_path, &frontmatter)?;
        summary.updated += 1;
        return Ok(next_view);
    }

    let (rel_path, abs_path) =
        pick_new_task_path(task_folder_abs, &input.vault_path, &remote.title)?;
    let body = body_for_new_task(remote);
    write_new_task_note(&abs_path, &frontmatter, &remote.title, &body)?;
    summary.created += 1;
    Ok(snapshot_view_from(remote, &rel_path))
}

fn snapshot_view_from(remote: &RemoteSummary, local_file_path: &str) -> SnapshotItem {
    SnapshotItem {
        item_id: remote.item_id.clone(),
        content_type: remote.content_type.clone(),
        title: remote.title.clone(),
        body: remote.body.clone(),
        url: remote.url.clone(),
        number: remote.number,
        repository: remote.repository.clone(),
        field_values: remote.field_values.clone(),
        local_file_path: local_file_path.to_string(),
    }
}

fn body_for_new_task(remote: &RemoteSummary) -> String {
    if let Some(body) = remote.body.as_deref() {
        return body.trim().to_string();
    }
    if let (Some(num), Some(repo), Some(url)) = (
        remote.number,
        remote.repository.as_deref(),
        remote.url.as_deref(),
    ) {
        return format!("[{repo}#{num}]({url})");
    }
    String::new()
}

/// Build the YAML mapping for one task note. We always rewrite the full set
/// of bridge-managed keys so a removed remote field disappears from local
/// frontmatter — the alternative (merge-only) would let stale values rot.
fn build_frontmatter(input: &PullInput, remote: &RemoteSummary) -> Mapping {
    let mut mapping = Mapping::new();
    set_str(&mut mapping, "type", "task");
    set_str(&mut mapping, "title", &remote.title);
    set_str(
        &mut mapping,
        "project",
        &format!("[[{}]]", input.project_note_stem),
    );
    set_str(
        &mut mapping,
        "github_project_node_id",
        &input.project_node_id,
    );
    set_str(&mut mapping, "github_item_id", &remote.item_id);
    set_str(&mut mapping, "github_content_type", &remote.content_type);
    if let (Some(num), Some(url)) = (remote.number, remote.url.as_deref()) {
        set_str(&mut mapping, "github_issue_url", url);
        mapping.insert(
            YamlValue::String("github_issue_number".into()),
            YamlValue::Number(serde_yaml::Number::from(num)),
        );
    }
    if let Some(repo) = remote.repository.as_deref() {
        set_str(&mut mapping, "github_issue_repo", repo);
    }
    set_str(&mut mapping, "github_last_synced", &input.now_rfc3339);

    if let Some(status_field) = input.status_field.as_deref() {
        if let Some(value) = remote.field_values.get(status_field) {
            set_str(&mut mapping, "status", value);
        }
    }
    for (local_key, github_key) in &input.field_mappings {
        if local_key == "status" {
            continue;
        }
        if let Some(value) = remote.field_values.get(github_key) {
            set_str(&mut mapping, local_key, value);
        }
    }
    mapping
}

fn set_str(mapping: &mut Mapping, key: &str, value: &str) {
    mapping.insert(
        YamlValue::String(key.to_string()),
        YamlValue::String(value.to_string()),
    );
}

fn pick_new_task_path(
    task_folder_abs: &Path,
    vault_path: &Path,
    title: &str,
) -> Result<(String, PathBuf), String> {
    let stem = title_slug(title);
    for attempt in 0..1000 {
        let candidate_name = if attempt == 0 {
            format!("{stem}.md")
        } else {
            format!("{stem}-{}.md", attempt + 1)
        };
        let candidate = task_folder_abs.join(&candidate_name);
        if !candidate.exists() {
            let rel = candidate
                .strip_prefix(vault_path)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| candidate.to_string_lossy().into_owned());
            return Ok((rel, candidate));
        }
    }
    Err(format!("Could not find a free filename for `{title}`."))
}

fn title_slug(title: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for ch in title.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed
    }
}

fn write_new_task_note(
    path: &Path,
    frontmatter: &Mapping,
    title: &str,
    body: &str,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create `{}`: {e}", parent.display()))?;
    }
    let content = render_note(frontmatter, title, body)?;
    fs::write(path, content).map_err(|e| format!("Failed to write task `{}`: {e}", path.display()))
}

/// Refresh frontmatter only — preserves whatever body the user has been
/// editing locally. See module docstring for the rationale.
fn update_existing_task_note(
    vault_path: &Path,
    rel_path: &str,
    next_frontmatter: &Mapping,
) -> Result<(), String> {
    let abs = vault_path.join(rel_path);
    let content = fs::read_to_string(&abs)
        .map_err(|e| format!("Failed to read task `{}`: {e}", abs.display()))?;
    let body = body_after_frontmatter(&content);
    let yaml = serde_yaml::to_string(&YamlValue::Mapping(next_frontmatter.clone()))
        .map_err(|e| format!("Failed to serialize frontmatter: {e}"))?;
    let yaml_trimmed = yaml.trim_end_matches('\n');
    let body_normalized = if body.is_empty() {
        String::new()
    } else {
        format!("\n{}", body.trim_start_matches('\n'))
    };
    let rebuilt = format!("---\n{yaml_trimmed}\n---{body_normalized}");
    fs::write(&abs, rebuilt).map_err(|e| format!("Failed to write task `{}`: {e}", abs.display()))
}

fn render_note(frontmatter: &Mapping, title: &str, body: &str) -> Result<String, String> {
    let yaml = serde_yaml::to_string(&YamlValue::Mapping(frontmatter.clone()))
        .map_err(|e| format!("Failed to serialize frontmatter: {e}"))?;
    let yaml_trimmed = yaml.trim_end_matches('\n');
    let body_block = if body.is_empty() {
        format!("\n# {title}\n")
    } else {
        format!("\n# {title}\n\n{}\n", body.trim_end_matches('\n'))
    };
    Ok(format!("---\n{yaml_trimmed}\n---{body_block}"))
}

fn body_after_frontmatter(content: &str) -> &str {
    let trimmed_start = content.trim_start();
    if !trimmed_start.starts_with("---") {
        return content;
    }
    let opener = content.find("---").expect("just checked the prefix");
    let after_open = &content[opener + 3..];
    let after_open = after_open.trim_start_matches('\r').trim_start_matches('\n');
    match after_open.find("\n---") {
        Some(close) => after_open[close + 4..]
            .trim_start_matches('\r')
            .trim_start_matches('\n'),
        None => content,
    }
}

fn delete_local_file(vault_path: &Path, rel_path: &str) -> Result<(), String> {
    let abs = vault_path.join(rel_path);
    match fs::remove_file(&abs) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Failed to delete `{}`: {e}", abs.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::projects::queries::{
        FieldRef, FieldValue, FieldValuesConnection, ProjectItem, ProjectItemContent, RepositoryRef,
    };
    use tempfile::TempDir;

    fn make_input(vault: &Path, cache_base: &Path, items: Vec<ProjectItem>) -> PullInput {
        PullInput {
            vault_path: vault.to_path_buf(),
            task_folder_rel: "tasks/q2".into(),
            project_node_id: "PVT_kw_demo".into(),
            project_note_stem: "q2-launch".into(),
            status_field: Some("Status".into()),
            field_mappings: vec![
                ("priority".into(), "Priority".into()),
                ("due".into(), "Due".into()),
            ],
            items,
            now_rfc3339: "2026-05-15T12:00:00Z".into(),
            cache_base: cache_base.to_path_buf(),
        }
    }

    struct Sandbox {
        vault: TempDir,
        cache: TempDir,
    }

    fn sandbox() -> Sandbox {
        Sandbox {
            vault: TempDir::new().unwrap(),
            cache: TempDir::new().unwrap(),
        }
    }

    fn draft_item(id: &str, title: &str, body: &str, fields: &[(&str, &str)]) -> ProjectItem {
        ProjectItem {
            id: id.into(),
            content: Some(ProjectItemContent::DraftIssue {
                title: title.into(),
                body: Some(body.into()),
            }),
            field_values: FieldValuesConnection {
                nodes: fields
                    .iter()
                    .map(|(name, value)| FieldValue::ProjectV2ItemFieldTextValue {
                        text: Some((*value).into()),
                        field: FieldRef {
                            id: format!("FIELD_{name}"),
                            name: (*name).into(),
                        },
                    })
                    .collect(),
            },
        }
    }

    fn issue_item(id: &str, number: i32, title: &str, repo: &str) -> ProjectItem {
        ProjectItem {
            id: id.into(),
            content: Some(ProjectItemContent::Issue {
                number,
                title: title.into(),
                url: format!("https://github.com/{repo}/issues/{number}"),
                repository: RepositoryRef {
                    name_with_owner: repo.into(),
                },
            }),
            field_values: FieldValuesConnection { nodes: Vec::new() },
        }
    }

    #[test]
    fn create_writes_new_task_note_with_frontmatter_and_body() {
        let sb = sandbox();
        let item = draft_item(
            "PVTI_1",
            "Implement board view",
            "Body line 1\nBody line 2",
            &[("Status", "In Progress"), ("Priority", "High")],
        );
        let input = make_input(sb.vault.path(), sb.cache.path(), vec![item]);
        let summary = pull(&input).unwrap();
        assert_eq!(summary.created, 1);
        assert_eq!(summary.updated, 0);
        assert_eq!(summary.deleted, 0);

        let path = sb.vault.path().join("tasks/q2/implement-board-view.md");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("type: task"));
        assert!(content.contains("title: Implement board view"));
        assert!(content.contains("project: '[[q2-launch]]'"));
        assert!(content.contains("github_item_id: PVTI_1"));
        assert!(content.contains("github_project_node_id: PVT_kw_demo"));
        assert!(content.contains("github_last_synced: 2026-05-15T12:00:00Z"));
        assert!(content.contains("status: In Progress"));
        assert!(content.contains("priority: High"));
        assert!(content.contains("# Implement board view"));
        assert!(content.contains("Body line 1"));
    }

    #[test]
    fn second_pull_with_identical_state_marks_items_unchanged() {
        let sb = sandbox();
        let item = draft_item("PVTI_1", "Same", "Body", &[("Status", "Todo")]);
        pull(&make_input(
            sb.vault.path(),
            sb.cache.path(),
            vec![item.clone()],
        ))
        .unwrap();

        let summary = pull(&make_input(sb.vault.path(), sb.cache.path(), vec![item])).unwrap();
        assert_eq!(summary.unchanged, 1);
        assert_eq!(summary.created, 0);
        assert_eq!(summary.updated, 0);
    }

    #[test]
    fn changed_remote_field_updates_existing_note_without_touching_body() {
        let sb = sandbox();
        let initial = draft_item("PVTI_1", "Same", "Original body", &[("Status", "Todo")]);
        pull(&make_input(sb.vault.path(), sb.cache.path(), vec![initial])).unwrap();

        let path = sb.vault.path().join("tasks/q2/same.md");
        let mut current = std::fs::read_to_string(&path).unwrap();
        current.push_str("\n## User-added section\n");
        std::fs::write(&path, &current).unwrap();

        let updated = draft_item("PVTI_1", "Same", "Original body", &[("Status", "Done")]);
        let summary = pull(&make_input(sb.vault.path(), sb.cache.path(), vec![updated])).unwrap();
        assert_eq!(summary.updated, 1);

        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("status: Done"));
        assert!(after.contains("## User-added section"));
    }

    #[test]
    fn missing_remote_item_deletes_its_local_file() {
        let sb = sandbox();
        let a = draft_item("PVTI_a", "Alpha", "a", &[]);
        let b = draft_item("PVTI_b", "Beta", "b", &[]);
        pull(&make_input(
            sb.vault.path(),
            sb.cache.path(),
            vec![a, b.clone()],
        ))
        .unwrap();

        let alpha = sb.vault.path().join("tasks/q2/alpha.md");
        let beta = sb.vault.path().join("tasks/q2/beta.md");
        assert!(alpha.exists());
        assert!(beta.exists());

        let summary = pull(&make_input(sb.vault.path(), sb.cache.path(), vec![b])).unwrap();
        assert_eq!(summary.deleted, 1);
        assert!(!alpha.exists());
        assert!(beta.exists());
    }

    #[test]
    fn filename_collisions_get_a_numeric_suffix() {
        let sb = sandbox();
        let a = draft_item("PVTI_a", "Same Title", "", &[]);
        let b = draft_item("PVTI_b", "Same Title", "", &[]);
        let summary = pull(&make_input(sb.vault.path(), sb.cache.path(), vec![a, b])).unwrap();
        assert_eq!(summary.created, 2);
        assert!(sb.vault.path().join("tasks/q2/same-title.md").exists());
        assert!(sb.vault.path().join("tasks/q2/same-title-2.md").exists());
    }

    #[test]
    fn issue_items_record_repository_and_url_in_frontmatter() {
        let sb = sandbox();
        let item = issue_item("PVTI_iss", 42, "Bug report", "seltzdesign/tolaria");
        pull(&make_input(sb.vault.path(), sb.cache.path(), vec![item])).unwrap();
        let path = sb.vault.path().join("tasks/q2/bug-report.md");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("github_issue_number: 42"));
        assert!(content.contains("github_issue_repo: seltzdesign/tolaria"));
        assert!(
            content.contains("github_issue_url: https://github.com/seltzdesign/tolaria/issues/42")
        );
        assert!(content.contains("[seltzdesign/tolaria#42]"));
    }

    #[test]
    fn snapshot_is_persisted_after_pull() {
        let sb = sandbox();
        let item = draft_item("PVTI_snap", "Snapshot me", "", &[]);
        pull(&make_input(sb.vault.path(), sb.cache.path(), vec![item])).unwrap();
        let snap = snapshot::load(sb.cache.path(), "PVT_kw_demo");
        assert_eq!(snap.items.len(), 1);
        assert!(snap.items.contains_key("PVTI_snap"));
        assert_eq!(snap.synced_at, "2026-05-15T12:00:00Z");
    }

    #[test]
    fn empty_remote_after_seeded_state_deletes_everything() {
        let sb = sandbox();
        let a = draft_item("PVTI_a", "Alpha", "", &[]);
        pull(&make_input(sb.vault.path(), sb.cache.path(), vec![a])).unwrap();
        assert!(sb.vault.path().join("tasks/q2/alpha.md").exists());

        let summary = pull(&make_input(sb.vault.path(), sb.cache.path(), Vec::new())).unwrap();
        assert_eq!(summary.deleted, 1);
        assert!(!sb.vault.path().join("tasks/q2/alpha.md").exists());
    }

    #[test]
    fn title_slug_handles_punctuation_and_whitespace() {
        assert_eq!(title_slug("Hello, World!"), "hello-world");
        assert_eq!(title_slug("   "), "untitled");
        assert_eq!(title_slug("A/B/C"), "a-b-c");
    }
}
