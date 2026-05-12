//! Typed views over [`VaultEntry`](super::entry::VaultEntry) for `type: task` and
//! `type: project` notes.
//!
//! Per [ADR 0115](../../../docs/adr/0115-tasks-and-projects-as-typed-notes.md), tasks
//! and projects are `VaultEntry` values with a specific `type` field and a locked
//! frontmatter schema. They do not have their own structs or storage path. The accessors
//! here read on demand from the entry's `properties` and `relationships` maps and parse
//! strings into typed values.

use super::date_or_datetime::DateOrDateTime;
use super::entry::VaultEntry;

/// Zero-cost wrapper around a task entry. Created via [`VaultEntry::as_task`].
pub struct TaskView<'a>(&'a VaultEntry);

/// Zero-cost wrapper around a project entry. Created via [`VaultEntry::as_project`].
pub struct ProjectView<'a>(&'a VaultEntry);

impl VaultEntry {
    /// True when this entry's `type` is `task`.
    pub fn is_task(&self) -> bool {
        self.is_a.as_deref() == Some("task")
    }

    /// True when this entry's `type` is `project`.
    pub fn is_project(&self) -> bool {
        self.is_a.as_deref() == Some("project")
    }

    /// Borrow this entry as a typed task view. Returns `None` if it is not a task.
    pub fn as_task(&self) -> Option<TaskView<'_>> {
        self.is_task().then_some(TaskView(self))
    }

    /// Borrow this entry as a typed project view. Returns `None` if it is not a project.
    pub fn as_project(&self) -> Option<ProjectView<'_>> {
        self.is_project().then_some(ProjectView(self))
    }
}

/// Extract a wikilink target from a single bracketed string. `"[[Foo]]"` → `"Foo"`,
/// `"[[Foo|Display]]"` → `"Foo"`. Returns `None` for unbracketed strings.
fn wikilink_target(s: &str) -> Option<String> {
    let inner = s.strip_prefix("[[").and_then(|r| r.strip_suffix("]]"))?;
    let target = inner.split_once('|').map_or(inner, |(t, _)| t);
    let trimmed = target.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn wikilink_targets(values: &[String]) -> Vec<String> {
    values.iter().filter_map(|s| wikilink_target(s)).collect()
}

fn property_str<'a>(entry: &'a VaultEntry, key: &str) -> Option<&'a str> {
    entry.properties.get(key)?.as_str()
}

fn property_strings(entry: &VaultEntry, key: &str) -> Vec<String> {
    match entry.properties.get(key) {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn relationship_targets(entry: &VaultEntry, key: &str) -> Vec<String> {
    entry
        .relationships
        .get(key)
        .map(|v| wikilink_targets(v))
        .unwrap_or_default()
}

impl<'a> TaskView<'a> {
    pub fn entry(&self) -> &'a VaultEntry {
        self.0
    }

    pub fn status(&self) -> Option<&'a str> {
        self.0.status.as_deref()
    }

    pub fn priority(&self) -> Option<&'a str> {
        property_str(self.0, "priority")
    }

    pub fn due(&self) -> Option<DateOrDateTime> {
        DateOrDateTime::parse(property_str(self.0, "due")?).ok()
    }

    pub fn start(&self) -> Option<DateOrDateTime> {
        DateOrDateTime::parse(property_str(self.0, "start")?).ok()
    }

    pub fn completed(&self) -> Option<DateOrDateTime> {
        DateOrDateTime::parse(property_str(self.0, "completed")?).ok()
    }

    pub fn estimate(&self) -> Option<f64> {
        self.0.properties.get("estimate")?.as_f64()
    }

    pub fn labels(&self) -> Vec<String> {
        property_strings(self.0, "labels")
    }

    pub fn project(&self) -> Option<String> {
        relationship_targets(self.0, "project").into_iter().next()
    }

    pub fn assignees(&self) -> Vec<String> {
        relationship_targets(self.0, "assignee")
    }

    pub fn blocked_by(&self) -> Vec<String> {
        relationship_targets(self.0, "blocked_by")
    }

    pub fn github_sync_status(&self) -> Option<&'a str> {
        property_str(self.0, "github_sync_status")
    }

    pub fn github_item_node_id(&self) -> Option<&'a str> {
        property_str(self.0, "github_item_node_id")
    }

    pub fn github_project_node_id(&self) -> Option<&'a str> {
        property_str(self.0, "github_project_node_id")
    }

    pub fn github_issue_url(&self) -> Option<&'a str> {
        property_str(self.0, "github_issue_url")
    }

    pub fn github_last_synced(&self) -> Option<&'a str> {
        property_str(self.0, "github_last_synced")
    }

    pub fn github_remote_snapshot_hash(&self) -> Option<&'a str> {
        property_str(self.0, "github_remote_snapshot_hash")
    }
}

impl<'a> ProjectView<'a> {
    pub fn entry(&self) -> &'a VaultEntry {
        self.0
    }

    pub fn task_folder(&self) -> Option<&'a str> {
        property_str(self.0, "task_folder")
    }

    pub fn statuses(&self) -> Vec<String> {
        property_strings(self.0, "statuses")
    }

    /// Per [ADR 0115 §3](../../../docs/adr/0115-tasks-and-projects-as-typed-notes.md),
    /// `terminal_statuses` defaults to `[Done]` if not set, or the last entry of
    /// `statuses` if no entry named `Done` is present.
    pub fn terminal_statuses(&self) -> Vec<String> {
        let explicit = property_strings(self.0, "terminal_statuses");
        if !explicit.is_empty() {
            return explicit;
        }
        let statuses = self.statuses();
        if statuses.iter().any(|s| s.eq_ignore_ascii_case("done")) {
            return vec!["Done".to_string()];
        }
        statuses
            .last()
            .cloned()
            .map(|s| vec![s])
            .unwrap_or_default()
    }

    pub fn default_view(&self) -> Option<&'a str> {
        property_str(self.0, "default_view")
    }

    pub fn sync_enabled(&self) -> bool {
        self.0
            .properties
            .get("sync_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    pub fn sync_interval_minutes(&self) -> u32 {
        self.0
            .properties
            .get("sync_interval_minutes")
            .and_then(|v| v.as_u64())
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(5)
    }

    pub fn link_to_issues(&self) -> bool {
        self.0
            .properties
            .get("link_to_issues")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    pub fn github_project_url(&self) -> Option<&'a str> {
        property_str(self.0, "github_project_url")
    }

    pub fn github_project_node_id(&self) -> Option<&'a str> {
        property_str(self.0, "github_project_node_id")
    }

    pub fn github_issue_repo(&self) -> Option<&'a str> {
        property_str(self.0, "github_issue_repo")
    }

    pub fn status_field(&self) -> Option<&'a str> {
        property_str(self.0, "status_field")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn task_entry() -> VaultEntry {
        let mut properties: HashMap<String, serde_json::Value> = HashMap::new();
        properties.insert("priority".into(), serde_json::json!("P1"));
        properties.insert("due".into(), serde_json::json!("2026-05-20"));
        properties.insert("start".into(), serde_json::json!("2026-05-15"));
        properties.insert("estimate".into(), serde_json::json!(3));
        properties.insert(
            "labels".into(),
            serde_json::json!(["bug", "frontend"]),
        );
        properties.insert("github_sync_status".into(), serde_json::json!("synced"));
        properties.insert("github_item_node_id".into(), serde_json::json!("PVTI_lAHO"));

        let mut relationships: HashMap<String, Vec<String>> = HashMap::new();
        relationships.insert("project".into(), vec!["[[My Cool Project]]".into()]);
        relationships.insert(
            "assignee".into(),
            vec!["[[Armin]]".into(), "[[Bob|Bobby]]".into()],
        );
        relationships.insert("blocked_by".into(), vec!["[[Set up CI]]".into()]);

        VaultEntry {
            is_a: Some("task".to_string()),
            status: Some("In progress".to_string()),
            properties,
            relationships,
            ..VaultEntry::default()
        }
    }

    fn project_entry() -> VaultEntry {
        let mut properties: HashMap<String, serde_json::Value> = HashMap::new();
        properties.insert("task_folder".into(), serde_json::json!("Projects/X/tasks"));
        properties.insert(
            "statuses".into(),
            serde_json::json!(["Not started", "In progress", "Done"]),
        );
        properties.insert("default_view".into(), serde_json::json!("board"));
        properties.insert("sync_enabled".into(), serde_json::json!(true));
        properties.insert("sync_interval_minutes".into(), serde_json::json!(10));

        VaultEntry {
            is_a: Some("project".to_string()),
            properties,
            ..VaultEntry::default()
        }
    }

    #[test]
    fn as_task_returns_some_only_for_task_type() {
        let task = task_entry();
        assert!(task.is_task());
        assert!(task.as_task().is_some());
        assert!(!task.is_project());
        assert!(task.as_project().is_none());

        let project = project_entry();
        assert!(project.is_project());
        assert!(project.as_project().is_some());
        assert!(!project.is_task());
        assert!(project.as_task().is_none());

        let plain = VaultEntry::default();
        assert!(plain.as_task().is_none());
        assert!(plain.as_project().is_none());
    }

    #[test]
    fn task_view_reads_scalar_properties() {
        let entry = task_entry();
        let task = entry.as_task().unwrap();
        assert_eq!(task.status(), Some("In progress"));
        assert_eq!(task.priority(), Some("P1"));
        assert_eq!(task.estimate(), Some(3.0));
        assert_eq!(task.github_sync_status(), Some("synced"));
        assert_eq!(task.github_item_node_id(), Some("PVTI_lAHO"));
    }

    #[test]
    fn task_view_parses_dates() {
        let entry = task_entry();
        let task = entry.as_task().unwrap();
        assert_eq!(task.due().unwrap().to_storage_string(), "2026-05-20");
        assert_eq!(task.start().unwrap().to_storage_string(), "2026-05-15");
        assert!(task.completed().is_none());
    }

    #[test]
    fn task_view_reads_labels_as_list() {
        let entry = task_entry();
        let task = entry.as_task().unwrap();
        assert_eq!(task.labels(), vec!["bug", "frontend"]);
    }

    #[test]
    fn task_view_extracts_wikilink_targets() {
        let entry = task_entry();
        let task = entry.as_task().unwrap();
        assert_eq!(task.project(), Some("My Cool Project".to_string()));
        assert_eq!(task.assignees(), vec!["Armin", "Bob"]);
        assert_eq!(task.blocked_by(), vec!["Set up CI"]);
    }

    #[test]
    fn task_view_handles_invalid_date_gracefully() {
        let mut entry = task_entry();
        entry
            .properties
            .insert("due".into(), serde_json::json!("not-a-date"));
        let task = entry.as_task().unwrap();
        assert!(task.due().is_none());
    }

    #[test]
    fn project_view_reads_basic_fields() {
        let entry = project_entry();
        let project = entry.as_project().unwrap();
        assert_eq!(project.task_folder(), Some("Projects/X/tasks"));
        assert_eq!(project.statuses(), vec!["Not started", "In progress", "Done"]);
        assert_eq!(project.default_view(), Some("board"));
        assert!(project.sync_enabled());
        assert_eq!(project.sync_interval_minutes(), 10);
    }

    #[test]
    fn project_view_terminal_statuses_explicit() {
        let mut entry = project_entry();
        entry.properties.insert(
            "terminal_statuses".into(),
            serde_json::json!(["Done", "Cancelled"]),
        );
        let project = entry.as_project().unwrap();
        assert_eq!(project.terminal_statuses(), vec!["Done", "Cancelled"]);
    }

    #[test]
    fn project_view_terminal_statuses_defaults_to_done_if_present() {
        let entry = project_entry(); // statuses includes "Done"
        let project = entry.as_project().unwrap();
        assert_eq!(project.terminal_statuses(), vec!["Done"]);
    }

    #[test]
    fn project_view_terminal_statuses_defaults_to_last_status_if_no_done() {
        let mut entry = project_entry();
        entry.properties.insert(
            "statuses".into(),
            serde_json::json!(["Open", "In review", "Closed"]),
        );
        let project = entry.as_project().unwrap();
        assert_eq!(project.terminal_statuses(), vec!["Closed"]);
    }

    #[test]
    fn project_view_sync_interval_defaults_to_5() {
        let mut entry = project_entry();
        entry.properties.remove("sync_interval_minutes");
        let project = entry.as_project().unwrap();
        assert_eq!(project.sync_interval_minutes(), 5);
    }

    #[test]
    fn project_view_link_to_issues_defaults_to_false() {
        let entry = project_entry();
        let project = entry.as_project().unwrap();
        assert!(!project.link_to_issues());
    }

    #[test]
    fn wikilink_target_handles_pipe_and_bare() {
        assert_eq!(wikilink_target("[[Foo]]").as_deref(), Some("Foo"));
        assert_eq!(wikilink_target("[[Foo|Bar]]").as_deref(), Some("Foo"));
        assert_eq!(wikilink_target("[[ Foo ]]").as_deref(), Some("Foo"));
        assert!(wikilink_target("Foo").is_none());
        assert!(wikilink_target("[[]]").is_none());
    }
}
