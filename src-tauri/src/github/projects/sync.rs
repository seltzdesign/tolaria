//! Pull-side of the GitHub Projects v2 bridge.
//!
//! Takes a snapshot of "what we last saw on github.com" (see [`super::snapshot`])
//! and a freshly fetched list of remote items, computes the create / update /
//! delete actions, and applies them to the bound project's task folder inside
//! the vault. Push-side (debounced field updates from local edits) lives in
//! [`super::push`] and runs after this module in the `github_sync` command.
//!
//! ## Body handling
//!
//! On *create*, we copy the remote draft body into the local task note so the
//! task starts with the same description GitHub has. On *update*, we deliberately
//! leave the body alone and only refresh frontmatter. The body is where local
//! work happens (subtasks, links, notes); overwriting it on every pull would be
//! hostile.
//!
//! ## Reconciliation (P13)
//!
//! On every existing item, the engine runs a per-field reconcile: for each
//! mapped field it knows about, it compares `local`, `snapshot`, and
//! `remote` and chooses one of four outcomes — nothing-changed, apply
//! remote, keep local, or conflict-LWW. Conflicts (local AND remote both
//! diverged from snapshot with different values) resolve by file mtime vs
//! `ProjectV2Item.updatedAt`, and the loser is filed at
//! `<cache>/github-sync/conflicts/<item>-<ts>.md` for manual recovery. The
//! local task gets a `github_sync_status: conflicted` flag so the UI can
//! surface it; the flag auto-clears on the next clean sync.

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
    /// Number of field-level conflicts the reconciler detected this
    /// cycle (one count per (item, field) pair). Each conflict is
    /// recorded in the cache-dir conflict log; the count just lets the
    /// UI show "you had N conflicts" in the result line.
    pub conflicts: u32,
    pub errors: Vec<String>,
}

/// Which side of a conflict won under last-write-wins. Recorded in the
/// conflict log + sync log so the user can see why a specific edit
/// landed (or got moved to the loser file).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictWinner {
    Local,
    Remote,
}

/// One field-level disagreement between local and remote that the
/// reconciler resolved. All four values are captured so the loser
/// file gives the user everything they need to manually recover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldConflict {
    pub field_name: String,
    pub local_value: String,
    pub remote_value: String,
    pub snapshot_value: String,
    pub winner: ConflictWinner,
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
    /// `ProjectV2Item.updatedAt` from the latest fetch. None when the
    /// API doesn't report it (legacy items, mocked tests).
    updated_at: Option<String>,
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
        updated_at: item.updated_at.clone(),
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
    if let Some(prev) = prior.items.get(&remote.item_id) {
        return reconcile_existing_item(input, prev, remote, summary);
    }

    let frontmatter = build_frontmatter(input, remote);
    let (rel_path, abs_path) =
        pick_new_task_path(task_folder_abs, &input.vault_path, &remote.title)?;
    let body = body_for_new_task(remote);
    write_new_task_note(&abs_path, &frontmatter, &remote.title, &body)?;
    summary.created += 1;
    Ok(snapshot_view_from(remote, &rel_path))
}

/// Fields where the reconciler decided local wins over remote.
/// The pull-side write still rebuilds frontmatter from remote, then
/// overrides these keys back to their local values so the user's edit
/// survives the cycle; the snapshot keeps the OLD value for these
/// fields so push sees a diff next cycle and propagates upstream.
struct LocalWinningField {
    gh_name: String,
    local_value: String,
}

/// Drive one existing-item update through the reconciler. Reads the
/// current local frontmatter, walks every field that's "interesting"
/// (snapshot, remote, or mapped), produces a `FieldDecision` per field,
/// and writes the merged result. Any field-level disagreement (both
/// sides changed since snapshot, with different values) is recorded as
/// a `FieldConflict` and resolved by LWW.
fn reconcile_existing_item(
    input: &PullInput,
    prev: &SnapshotItem,
    remote: &RemoteSummary,
    summary: &mut PullSummary,
) -> Result<SnapshotItem, String> {
    let local_abs = input.vault_path.join(&prev.local_file_path);
    let local_content = fs::read_to_string(&local_abs).unwrap_or_default();
    let (local_fm, _local_body) = match split_frontmatter_loose(&local_content) {
        Ok(parts) => parts,
        Err(_) => (Mapping::new(), local_content.as_str()),
    };
    let local_mtime_rfc = read_file_mtime_rfc3339(&local_abs);

    let local_field_values = collect_local_fields(
        &local_fm,
        input.status_field.as_deref(),
        &input.field_mappings,
    );
    let mut local_winners: Vec<LocalWinningField> = Vec::new();
    let mut conflicts: Vec<FieldConflict> = Vec::new();
    let mut next_field_values = prev.field_values.clone();
    let mut any_change = false;

    let field_names = field_names_to_consider(prev, remote, input);
    for gh_name in field_names {
        let snap_val = prev.field_values.get(&gh_name).cloned().unwrap_or_default();
        let remote_val = remote
            .field_values
            .get(&gh_name)
            .cloned()
            .unwrap_or_default();
        let local_val = local_field_values
            .get(&gh_name)
            .cloned()
            .unwrap_or_default();
        let local_changed = local_val != snap_val;
        let remote_changed = remote_val != snap_val;
        match (local_changed, remote_changed) {
            (false, false) => {}
            (false, true) => {
                next_field_values.insert(gh_name.clone(), remote_val.clone());
                any_change = true;
            }
            (true, false) => {
                local_winners.push(LocalWinningField {
                    gh_name,
                    local_value: local_val,
                });
                any_change = true;
            }
            (true, true) => {
                if local_val == remote_val {
                    next_field_values.insert(gh_name.clone(), remote_val.clone());
                    any_change = true;
                } else {
                    let winner =
                        decide_lww(local_mtime_rfc.as_deref(), remote.updated_at.as_deref());
                    conflicts.push(FieldConflict {
                        field_name: gh_name.clone(),
                        local_value: local_val.clone(),
                        remote_value: remote_val.clone(),
                        snapshot_value: snap_val.clone(),
                        winner,
                    });
                    match winner {
                        ConflictWinner::Local => {
                            local_winners.push(LocalWinningField {
                                gh_name,
                                local_value: local_val,
                            });
                        }
                        ConflictWinner::Remote => {
                            next_field_values.insert(gh_name.clone(), remote_val.clone());
                        }
                    }
                    any_change = true;
                }
            }
        }
    }

    // Build the next snapshot view: identity fields from the fresh remote
    // summary (title, content type, etc.), field values from the
    // reconcile output. `remote_updated_at` always tracks the latest
    // remote timestamp so the next cycle's LWW sees a current baseline.
    let mut next_view = snapshot_view_from(remote, &prev.local_file_path);
    next_view.field_values = next_field_values;

    let identity_unchanged = next_view.title == prev.title
        && next_view.body == prev.body
        && next_view.url == prev.url
        && next_view.number == prev.number
        && next_view.repository == prev.repository;
    let nothing_to_do = !any_change && identity_unchanged;
    if nothing_to_do {
        summary.unchanged += 1;
        return Ok(next_view);
    }

    // The local file write merges decisions: start from a remote-driven
    // frontmatter and override the keys where local wins.
    let mut frontmatter = build_frontmatter(input, remote);
    for winner in &local_winners {
        if let Some(local_key) = local_key_for_github_field(input, &winner.gh_name) {
            frontmatter.insert(
                YamlValue::String(local_key),
                yaml_value_from_string(&winner.local_value),
            );
        }
    }
    if !conflicts.is_empty() {
        frontmatter.insert(
            YamlValue::String("github_sync_status".into()),
            YamlValue::String("conflicted".into()),
        );
    }

    update_existing_task_note(&input.vault_path, &prev.local_file_path, &frontmatter)?;

    if !conflicts.is_empty() {
        write_conflict_record(input, prev, remote, &conflicts)?;
        summary.conflicts += conflicts.len() as u32;
    }
    summary.updated += 1;
    Ok(next_view)
}

/// Set of GitHub-field names we need to consider for this item:
/// everything the snapshot tracked, plus everything remote returned,
/// plus the GH-side names of every mapped local key. We intersect
/// with mapped fields below for the actual conflict logic — the
/// expanded set just makes sure we don't skip a field that's in the
/// snapshot but not in the latest remote response (which would mean
/// the value was cleared upstream).
fn field_names_to_consider(
    prev: &SnapshotItem,
    remote: &RemoteSummary,
    input: &PullInput,
) -> Vec<String> {
    let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    set.extend(prev.field_values.keys().cloned());
    set.extend(remote.field_values.keys().cloned());
    if let Some(status_field) = input.status_field.as_deref() {
        set.insert(status_field.to_string());
    }
    for (_local, gh_name) in &input.field_mappings {
        set.insert(gh_name.clone());
    }
    set.into_iter().collect()
}

/// Inverse-lookup helper: given a GitHub field name, find the local
/// frontmatter key it maps to. Returns None for unknown / unmapped
/// fields (those bypass the local-wins write path).
fn local_key_for_github_field(input: &PullInput, gh_name: &str) -> Option<String> {
    if let Some(status_field) = input.status_field.as_deref() {
        if status_field == gh_name {
            return Some("status".into());
        }
    }
    input
        .field_mappings
        .iter()
        .find(|(_local, gh)| gh == gh_name)
        .map(|(local, _gh)| local.clone())
}

/// Read the local task's current field values, keyed by GitHub field
/// name so they line up with the snapshot. Numbers/booleans normalise
/// to the same string form the snapshot stores so a value that lives
/// as YAML `5` compares equal to the snapshot's `"5"`.
fn collect_local_fields(
    frontmatter: &Mapping,
    status_field: Option<&str>,
    field_mappings: &[(String, String)],
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(status_field) = status_field {
        if let Some(value) = frontmatter_value_string(frontmatter, "status") {
            out.insert(status_field.to_string(), value);
        }
    }
    for (local_key, gh_name) in field_mappings {
        if local_key == "status" {
            continue;
        }
        if let Some(value) = frontmatter_value_string(frontmatter, local_key) {
            out.insert(gh_name.clone(), value);
        }
    }
    out
}

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

/// LWW decision: compare the local file's mtime against the remote
/// item's `updatedAt`. Most-recent wins. When either timestamp is
/// missing or unparseable, we fall back to remote-wins — that's the
/// safer default for a sync that's mostly downstream-driven and it
/// matches the behaviour the engine had before P13.
fn decide_lww(local_mtime: Option<&str>, remote_updated_at: Option<&str>) -> ConflictWinner {
    let local = local_mtime.and_then(parse_rfc3339);
    let remote = remote_updated_at.and_then(parse_rfc3339);
    match (local, remote) {
        (Some(l), Some(r)) if l > r => ConflictWinner::Local,
        _ => ConflictWinner::Remote,
    }
}

fn parse_rfc3339(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

fn read_file_mtime_rfc3339(path: &Path) -> Option<String> {
    let modified = fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let datetime: chrono::DateTime<chrono::Utc> = modified.into();
    Some(datetime.to_rfc3339())
}

fn split_frontmatter_loose(content: &str) -> Result<(Mapping, &str), String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok((Mapping::new(), content));
    }
    let opener = content.find("---").expect("just checked the prefix");
    let after_open = &content[opener + 3..];
    let after_open = after_open.trim_start_matches('\r').trim_start_matches('\n');
    let close = after_open
        .find("\n---")
        .ok_or_else(|| "frontmatter unterminated".to_string())?;
    let fm_text = &after_open[..close];
    let body = after_open[close + 4..]
        .trim_start_matches('\r')
        .trim_start_matches('\n');
    let mapping: Mapping = if fm_text.trim().is_empty() {
        Mapping::new()
    } else {
        serde_yaml::from_str(fm_text).map_err(|e| e.to_string())?
    };
    Ok((mapping, body))
}

/// Persist a per-conflict markdown record under
/// `<cache>/github-sync/conflicts/<item_id>-<timestamp>.md`. The file
/// is meant to be human-readable: it lists every conflicted field, who
/// won under LWW, and both candidate values so the user can manually
/// recover whichever side lost.
fn write_conflict_record(
    input: &PullInput,
    prev: &SnapshotItem,
    remote: &RemoteSummary,
    conflicts: &[FieldConflict],
) -> Result<(), String> {
    let dir = input.cache_base.join("conflicts");
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create conflict dir `{}`: {e}", dir.display()))?;
    let sanitized_id: String = prev
        .item_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let ts = input.now_rfc3339.replace([':', '.'], "-");
    let path = dir.join(format!("{sanitized_id}-{ts}.md"));
    let mut content = String::new();
    content.push_str(&format!("# Conflict — {}\n\n", remote.title));
    content.push_str(&format!("- Item: `{}`\n", prev.item_id));
    content.push_str(&format!("- Local file: `{}`\n", prev.local_file_path));
    content.push_str(&format!("- Detected at: {}\n", input.now_rfc3339));
    content.push('\n');
    for field in conflicts {
        let winner = match field.winner {
            ConflictWinner::Local => "local",
            ConflictWinner::Remote => "remote",
        };
        content.push_str(&format!(
            "## Field `{}` (winner: {winner})\n",
            field.field_name
        ));
        content.push_str(&format!("- Local value: `{}`\n", field.local_value));
        content.push_str(&format!("- Remote value: `{}`\n", field.remote_value));
        content.push_str(&format!("- Snapshot value: `{}`\n\n", field.snapshot_value));
    }
    fs::write(&path, content)
        .map_err(|e| format!("Failed to write conflict record `{}`: {e}", path.display()))
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
        remote_updated_at: remote.updated_at.clone(),
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
            mapping.insert(
                YamlValue::String(local_key.clone()),
                yaml_value_from_string(value),
            );
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

/// Promote a raw field-value string into the YAML type Tolaria's task UI
/// expects. Numeric fields like Estimate live as `BTreeMap<String, String>`
/// in the snapshot for diff stability, but the local frontmatter must emit
/// them as YAML numbers — Tolaria's `propertyNumber` reader rejects strings
/// and the Estimate cell would otherwise render empty. Anything that doesn't
/// parse as a number stays a string (dates, statuses, free text).
fn yaml_value_from_string(value: &str) -> YamlValue {
    if let Ok(n) = value.parse::<i64>() {
        return YamlValue::Number(serde_yaml::Number::from(n));
    }
    if let Ok(n) = value.parse::<f64>() {
        if n.is_finite() {
            return YamlValue::Number(serde_yaml::Number::from(n));
        }
    }
    YamlValue::String(value.to_string())
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
            updated_at: None,
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

    fn item_with_number_field(id: &str, title: &str, field_name: &str, value: f64) -> ProjectItem {
        ProjectItem {
            id: id.into(),
            updated_at: None,
            content: Some(ProjectItemContent::DraftIssue {
                title: title.into(),
                body: None,
            }),
            field_values: FieldValuesConnection {
                nodes: vec![FieldValue::ProjectV2ItemFieldNumberValue {
                    number: Some(value),
                    field: FieldRef {
                        id: format!("FIELD_{field_name}"),
                        name: field_name.into(),
                    },
                }],
            },
        }
    }

    fn issue_item(id: &str, number: i32, title: &str, repo: &str) -> ProjectItem {
        ProjectItem {
            id: id.into(),
            updated_at: None,
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

    #[test]
    fn number_fields_land_as_yaml_numbers_not_quoted_strings() {
        let sb = sandbox();
        let mut input = make_input(
            sb.vault.path(),
            sb.cache.path(),
            vec![item_with_number_field(
                "PVTI_n",
                "Estimated work",
                "Estimate",
                5.0,
            )],
        );
        input.field_mappings = vec![("estimate".into(), "Estimate".into())];
        pull(&input).unwrap();
        let content =
            std::fs::read_to_string(sb.vault.path().join("tasks/q2/estimated-work.md")).unwrap();
        assert!(
            content.contains("estimate: 5\n"),
            "expected unquoted YAML number, got:\n{content}"
        );
    }

    #[test]
    fn yaml_value_promotion_recognizes_ints_floats_and_falls_back_to_string() {
        assert_eq!(
            yaml_value_from_string("42"),
            YamlValue::Number(serde_yaml::Number::from(42i64))
        );
        match yaml_value_from_string("3.5") {
            YamlValue::Number(_) => {}
            other => panic!("expected number for `3.5`, got {other:?}"),
        }
        assert_eq!(
            yaml_value_from_string("In Progress"),
            YamlValue::String("In Progress".into())
        );
    }

    fn item_with_status(
        id: &str,
        title: &str,
        status: &str,
        updated_at: Option<&str>,
    ) -> ProjectItem {
        ProjectItem {
            id: id.into(),
            updated_at: updated_at.map(|s| s.to_string()),
            content: Some(ProjectItemContent::DraftIssue {
                title: title.into(),
                body: None,
            }),
            field_values: FieldValuesConnection {
                nodes: vec![FieldValue::ProjectV2ItemFieldTextValue {
                    text: Some(status.into()),
                    field: FieldRef {
                        id: "FIELD_status".into(),
                        name: "Status".into(),
                    },
                }],
            },
        }
    }

    fn rewrite_local_field(path: &Path, key: &str, value: &str) {
        let content = std::fs::read_to_string(path).unwrap();
        let mut next = String::new();
        let mut replaced = false;
        for line in content.lines() {
            if !replaced && line.starts_with(&format!("{key}:")) {
                next.push_str(&format!("{key}: {value}\n"));
                replaced = true;
            } else {
                next.push_str(line);
                next.push('\n');
            }
        }
        std::fs::write(path, next).unwrap();
    }

    #[test]
    fn local_only_change_survives_a_pull_with_no_remote_edits() {
        let sb = sandbox();
        let seed = item_with_status("PVTI_1", "Local edit", "Todo", Some("2026-05-15T10:00:00Z"));
        pull(&make_input(
            sb.vault.path(),
            sb.cache.path(),
            vec![seed.clone()],
        ))
        .unwrap();

        let task_path = sb.vault.path().join("tasks/q2/local-edit.md");
        rewrite_local_field(&task_path, "status", "In Progress");

        // Remote re-sends the SAME value it had before — local change should
        // be left alone and survive the cycle.
        let summary = pull(&make_input(sb.vault.path(), sb.cache.path(), vec![seed])).unwrap();
        assert_eq!(summary.conflicts, 0);
        let after = std::fs::read_to_string(&task_path).unwrap();
        assert!(
            after.contains("status: In Progress"),
            "expected local edit preserved, got:\n{after}"
        );
    }

    #[test]
    fn conflict_with_remote_in_the_future_lets_remote_win_and_files_loser_in_cache() {
        let sb = sandbox();
        let seed = item_with_status("PVTI_1", "Both edit", "Todo", Some("2026-05-15T10:00:00Z"));
        pull(&make_input(sb.vault.path(), sb.cache.path(), vec![seed])).unwrap();

        let task_path = sb.vault.path().join("tasks/q2/both-edit.md");
        rewrite_local_field(&task_path, "status", "In Progress");

        let remote_newer =
            item_with_status("PVTI_1", "Both edit", "Done", Some("9999-12-31T23:59:59Z"));
        let summary = pull(&make_input(
            sb.vault.path(),
            sb.cache.path(),
            vec![remote_newer],
        ))
        .unwrap();
        assert_eq!(summary.conflicts, 1);

        let after = std::fs::read_to_string(&task_path).unwrap();
        assert!(
            after.contains("status: Done"),
            "remote should have won: {after}"
        );
        assert!(
            after.contains("github_sync_status: conflicted"),
            "expected conflicted status flag: {after}"
        );

        let conflicts_dir = sb.cache.path().join("conflicts");
        let entries: Vec<_> = std::fs::read_dir(&conflicts_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1);
        let conflict_doc = std::fs::read_to_string(entries[0].path()).unwrap();
        assert!(conflict_doc.contains("Local value: `In Progress`"));
        assert!(conflict_doc.contains("Remote value: `Done`"));
        assert!(conflict_doc.contains("winner: remote"));
    }

    #[test]
    fn conflict_with_remote_in_the_past_lets_local_win_and_keeps_snapshot_stale() {
        let sb = sandbox();
        let seed = item_with_status("PVTI_1", "Both edit", "Todo", Some("2026-05-15T10:00:00Z"));
        pull(&make_input(sb.vault.path(), sb.cache.path(), vec![seed])).unwrap();

        let task_path = sb.vault.path().join("tasks/q2/both-edit.md");
        rewrite_local_field(&task_path, "status", "In Progress");

        let remote_older =
            item_with_status("PVTI_1", "Both edit", "Done", Some("1970-01-01T00:00:00Z"));
        let summary = pull(&make_input(
            sb.vault.path(),
            sb.cache.path(),
            vec![remote_older],
        ))
        .unwrap();
        assert_eq!(summary.conflicts, 1);

        let after = std::fs::read_to_string(&task_path).unwrap();
        assert!(
            after.contains("status: In Progress"),
            "local should have won: {after}"
        );
        assert!(after.contains("github_sync_status: conflicted"));

        let snap = snapshot::load(sb.cache.path(), "PVT_kw_demo");
        let item = snap.items.get("PVTI_1").expect("item still in snapshot");
        // Snapshot must keep the OLD value so a follow-up push picks up
        // the local edit as a real diff.
        assert_eq!(
            item.field_values.get("Status").map(|s| s.as_str()),
            Some("Todo")
        );
    }

    #[test]
    fn decide_lww_prefers_remote_when_timestamps_are_missing() {
        assert_eq!(decide_lww(None, None), ConflictWinner::Remote);
        assert_eq!(
            decide_lww(None, Some("2026-05-15T10:00:00Z")),
            ConflictWinner::Remote
        );
    }

    #[test]
    fn decide_lww_picks_local_when_file_mtime_is_newer_than_remote() {
        assert_eq!(
            decide_lww(Some("2026-05-15T11:00:00Z"), Some("2026-05-15T10:00:00Z")),
            ConflictWinner::Local
        );
    }

    #[test]
    fn decide_lww_picks_remote_on_equal_timestamps() {
        let stamp = "2026-05-15T10:00:00Z";
        assert_eq!(decide_lww(Some(stamp), Some(stamp)), ConflictWinner::Remote);
    }
}
