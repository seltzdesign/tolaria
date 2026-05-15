//! Per-project snapshot of remote GitHub Projects v2 state.
//!
//! The snapshot is what we believe github.com looked like at the end of the
//! last successful sync. Diffing remote items against it lets the pull engine
//! decide which local task files to create, update, or delete without needing
//! a stable diff cursor on GitHub's side.
//!
//! Storage lives under `<cache>/github-sync/<project_node_id>.json`
//! ([ADR 0024](../../../docs/adr/0024-cache-outside-vault.md)): outside the
//! vault directory so it never appears in git status. Writes go through a
//! temp file + atomic rename so a crash mid-write cannot corrupt the file.
//!
//! Callers thread the cache base directory through `PullInput` so tests can
//! point at a tempdir without touching shared global state — env-var-based
//! cache redirection deadlocked parallel tests in P11.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const SNAPSHOT_VERSION: u32 = 1;

/// Snapshot of one bound project's remote state at the end of the last pull.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSnapshot {
    pub version: u32,
    pub project_node_id: String,
    pub synced_at: String,
    /// Item node id → last-seen remote state. BTreeMap so the on-disk JSON
    /// has a stable key order — easier to inspect and to assert on in tests.
    pub items: BTreeMap<String, SnapshotItem>,
}

impl ProjectSnapshot {
    pub fn empty(project_node_id: &str) -> Self {
        Self {
            version: SNAPSHOT_VERSION,
            project_node_id: project_node_id.to_string(),
            synced_at: String::new(),
            items: BTreeMap::new(),
        }
    }
}

/// One item's last-seen remote state. The `local_file_path` is vault-relative
/// (forward slashes) so we can locate and overwrite/delete the local task
/// note on the next pull. `field_values` is keyed by GitHub field *name*
/// (not id) because the bindings UI exposes names — keeping it human-readable
/// also makes the snapshot easier to debug from disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotItem {
    pub item_id: String,
    pub content_type: String,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub number: Option<i32>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub field_values: BTreeMap<String, String>,
    /// Last `ProjectV2Item.updatedAt` we observed from GitHub. Carried
    /// through so the reconciler can compare against the local file's
    /// mtime when both sides diverge — that's the LWW input.
    #[serde(default)]
    pub remote_updated_at: Option<String>,
    pub local_file_path: String,
}

/// Default cache root for production. `~/.laputa/cache/github-sync`.
/// The Tauri command resolves this at the boundary; everything below
/// receives an explicit `base` so tests can isolate.
pub fn default_base() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".laputa")
        .join("cache")
        .join("github-sync")
}

pub fn snapshot_path(base: &Path, project_node_id: &str) -> PathBuf {
    base.join(format!("{}.json", sanitize_node_id(project_node_id)))
}

/// Load the snapshot for a project, or an empty snapshot if none exists yet.
/// Corrupted on-disk files start fresh too — losing the diff baseline costs
/// one redundant create-on-pull cycle, far cheaper than refusing to sync.
pub fn load(base: &Path, project_node_id: &str) -> ProjectSnapshot {
    let path = snapshot_path(base, project_node_id);
    let Ok(content) = fs::read_to_string(&path) else {
        return ProjectSnapshot::empty(project_node_id);
    };
    serde_json::from_str(&content).unwrap_or_else(|_| ProjectSnapshot::empty(project_node_id))
}

/// Persist a snapshot atomically. The parent directory is created on demand
/// since the very first sync produces the very first snapshot.
pub fn save(base: &Path, snapshot: &ProjectSnapshot) -> Result<(), String> {
    let path = snapshot_path(base, &snapshot.project_node_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create snapshot dir `{}`: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(snapshot)
        .map_err(|e| format!("Failed to serialize snapshot: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    {
        let mut handle = fs::File::create(&tmp)
            .map_err(|e| format!("Failed to open snapshot tmp `{}`: {e}", tmp.display()))?;
        handle
            .write_all(json.as_bytes())
            .map_err(|e| format!("Failed to write snapshot tmp `{}`: {e}", tmp.display()))?;
        handle
            .sync_all()
            .map_err(|e| format!("Failed to fsync snapshot tmp `{}`: {e}", tmp.display()))?;
    }
    fs::rename(&tmp, &path)
        .map_err(|e| format!("Failed to commit snapshot `{}`: {e}", path.display()))
}

/// Project node ids are opaque to us but include characters like `=` that
/// some filesystems handle poorly. Replace anything that isn't a letter,
/// digit, dash, or underscore — collisions don't matter in practice because
/// real node ids only use the safe charset.
fn sanitize_node_id(node_id: &str) -> String {
    node_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_item() -> SnapshotItem {
        SnapshotItem {
            item_id: "PVTI_abc".into(),
            content_type: "DraftIssue".into(),
            title: "Implement board view".into(),
            body: Some("Body text".into()),
            url: None,
            number: None,
            repository: None,
            field_values: {
                let mut map = BTreeMap::new();
                map.insert("Status".into(), "In Progress".into());
                map.insert("Priority".into(), "High".into());
                map
            },
            remote_updated_at: None,
            local_file_path: "tasks/q2/implement-board-view.md".into(),
        }
    }

    #[test]
    fn load_returns_empty_when_no_file_exists() {
        let dir = TempDir::new().unwrap();
        let snap = load(dir.path(), "PVT_kw_missing");
        assert_eq!(snap.project_node_id, "PVT_kw_missing");
        assert!(snap.items.is_empty());
        assert_eq!(snap.version, SNAPSHOT_VERSION);
    }

    #[test]
    fn save_then_load_roundtrips_items() {
        let dir = TempDir::new().unwrap();
        let mut snap = ProjectSnapshot::empty("PVT_kw_roundtrip");
        snap.synced_at = "2026-05-15T10:00:00Z".into();
        snap.items.insert("PVTI_abc".into(), sample_item());
        save(dir.path(), &snap).unwrap();

        let loaded = load(dir.path(), "PVT_kw_roundtrip");
        assert_eq!(loaded.synced_at, "2026-05-15T10:00:00Z");
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items.get("PVTI_abc"), Some(&sample_item()));
    }

    #[test]
    fn save_creates_the_snapshot_directory_on_demand() {
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("nested/cache");
        let snap = ProjectSnapshot::empty("PVT_kw_dir");
        save(&base, &snap).unwrap();
        assert!(base.is_dir());
    }

    #[test]
    fn sanitize_replaces_unsafe_characters() {
        assert_eq!(sanitize_node_id("PVT_kw=abc/def"), "PVT_kw_abc_def");
    }

    #[test]
    fn load_recovers_from_a_corrupted_file_by_returning_empty() {
        let dir = TempDir::new().unwrap();
        let path = snapshot_path(dir.path(), "PVT_corrupt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not valid json {").unwrap();
        let snap = load(dir.path(), "PVT_corrupt");
        assert!(snap.items.is_empty());
    }
}
