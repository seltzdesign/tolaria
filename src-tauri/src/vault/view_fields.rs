//! Field-namespace resolution for view filter / group_by / columns fields.
//!
//! Supports three prefixes:
//! - `note.<name>` — frontmatter property (or one of the structural aliases
//!   `type`, `isA`, `status`, `title`, `body`). Falls back to a relationship
//!   lookup if no scalar property matches.
//! - `file.<name>` — entry / filesystem metadata. Locked v1 names:
//!   `name`, `path`, `folder`, `ext`, `size`, `ctime`, `mtime`, `tags`.
//! - `formula.<name>` — reserved for v2. Always resolves to `Scalar(None)`
//!   and emits a one-shot debug warning.
//!
//! Bare names (e.g. `priority`) are treated as `note.priority` for backwards
//! compatibility with view files that predate the namespace syntax.

use std::path::Path;
use std::sync::OnceLock;

use chrono::{TimeZone, Utc};

use super::view_value_conversions::json_scalar_to_string;
use super::VaultEntry;

#[derive(Debug)]
pub(super) enum ConditionField<'a> {
    Scalar(Option<String>),
    Relationship(&'a [String]),
}

pub(super) fn resolve_field<'a>(field: &str, entry: &'a VaultEntry) -> ConditionField<'a> {
    let (namespace, name) = match field.split_once('.') {
        Some((ns, rest)) if matches!(ns, "note" | "file" | "formula") => (ns, rest),
        _ => ("note", field),
    };
    match namespace {
        "file" => resolve_file_field(name, entry),
        "formula" => formula_field_placeholder(name),
        _ => resolve_note_field(name, entry),
    }
}

/// Boolean view fields, including their namespaced forms (`note.X`).
/// Returns the bare field name when the input matches an alias, so callers
/// can map cleanly to the underlying VaultEntry boolean.
pub(super) fn boolean_field_alias(field: &str) -> Option<&'static str> {
    match field {
        "archived" | "note.archived" => Some("archived"),
        "favorite" | "note.favorite" => Some("favorite"),
        _ => None,
    }
}

fn resolve_note_field<'a>(name: &str, entry: &'a VaultEntry) -> ConditionField<'a> {
    match name {
        "type" | "isA" => ConditionField::Scalar(entry.is_a.clone()),
        "status" => ConditionField::Scalar(entry.status.clone()),
        "title" => ConditionField::Scalar(Some(entry.title.clone())),
        "body" => ConditionField::Scalar(Some(entry.snippet.clone())),
        _ => resolve_dynamic_note_field(name, entry),
    }
}

fn resolve_dynamic_note_field<'a>(name: &str, entry: &'a VaultEntry) -> ConditionField<'a> {
    if let Some(prop) = entry.properties.get(name) {
        return ConditionField::Scalar(json_scalar_to_string(prop));
    }
    if let Some(relationships) = entry.relationships.get(name) {
        return ConditionField::Relationship(relationships);
    }
    ConditionField::Scalar(None)
}

fn resolve_file_field<'a>(name: &str, entry: &'a VaultEntry) -> ConditionField<'a> {
    match name {
        "name" => ConditionField::Scalar(Some(file_name_for(entry))),
        "path" => ConditionField::Scalar(Some(entry.path.clone())),
        "folder" => ConditionField::Scalar(file_folder_for(&entry.path)),
        "ext" => ConditionField::Scalar(file_extension_for(&entry.filename)),
        "size" => ConditionField::Scalar(Some(entry.file_size.to_string())),
        "ctime" => ConditionField::Scalar(epoch_millis_to_iso(entry.created_at)),
        "mtime" => ConditionField::Scalar(epoch_millis_to_iso(entry.modified_at)),
        "tags" => ConditionField::Relationship(&entry.belongs_to),
        _ => ConditionField::Scalar(None),
    }
}

fn file_name_for(entry: &VaultEntry) -> String {
    if !entry.title.is_empty() {
        return entry.title.clone();
    }
    Path::new(&entry.filename)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| entry.filename.clone())
}

fn file_folder_for(path: &str) -> Option<String> {
    let parent = Path::new(path).parent()?;
    let folder = parent.to_string_lossy();
    if folder.is_empty() {
        None
    } else {
        Some(folder.into_owned())
    }
}

fn file_extension_for(filename: &str) -> Option<String> {
    Path::new(filename)
        .extension()
        .map(|ext| ext.to_string_lossy().into_owned())
}

fn epoch_millis_to_iso(value: Option<u64>) -> Option<String> {
    let millis = value?;
    let secs = i64::try_from(millis / 1000).ok()?;
    let nsec = u32::try_from((millis % 1000) * 1_000_000).ok()?;
    Utc.timestamp_opt(secs, nsec)
        .single()
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S").to_string())
}

fn formula_field_placeholder<'a>(name: &str) -> ConditionField<'a> {
    static WARNED: OnceLock<()> = OnceLock::new();
    WARNED.get_or_init(|| {
        log::debug!(
            "formula.{} requested but formula.* fields are a v2 feature; resolving as empty",
            name
        );
    });
    ConditionField::Scalar(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_entry(overrides: impl FnOnce(&mut VaultEntry)) -> VaultEntry {
        let mut entry = VaultEntry::default();
        overrides(&mut entry);
        entry
    }

    fn scalar_string(field: ConditionField<'_>) -> Option<String> {
        match field {
            ConditionField::Scalar(value) => value,
            ConditionField::Relationship(_) => None,
        }
    }

    fn relationship_targets<'a>(field: ConditionField<'a>) -> Option<&'a [String]> {
        match field {
            ConditionField::Relationship(targets) => Some(targets),
            ConditionField::Scalar(_) => None,
        }
    }

    #[test]
    fn bare_field_resolves_as_note_namespace() {
        let entry = make_entry(|e| {
            let mut props: HashMap<String, serde_json::Value> = HashMap::new();
            props.insert("priority".into(), serde_json::json!("P1"));
            e.properties = props;
        });
        assert_eq!(scalar_string(resolve_field("priority", &entry)).as_deref(), Some("P1"));
        assert_eq!(
            scalar_string(resolve_field("note.priority", &entry)).as_deref(),
            Some("P1"),
        );
    }

    #[test]
    fn structural_alias_keeps_resolving_under_note_namespace() {
        let entry = make_entry(|e| {
            e.is_a = Some("Project".into());
            e.status = Some("In progress".into());
        });
        assert_eq!(scalar_string(resolve_field("type", &entry)).as_deref(), Some("Project"));
        assert_eq!(scalar_string(resolve_field("note.type", &entry)).as_deref(), Some("Project"));
        assert_eq!(
            scalar_string(resolve_field("note.status", &entry)).as_deref(),
            Some("In progress"),
        );
    }

    #[test]
    fn file_path_and_folder_and_name() {
        let entry = make_entry(|e| {
            e.path = "/vault/Projects/Active/launch.md".into();
            e.filename = "launch.md".into();
            e.title = "Launch".into();
        });
        assert_eq!(
            scalar_string(resolve_field("file.path", &entry)).as_deref(),
            Some("/vault/Projects/Active/launch.md"),
        );
        assert_eq!(
            scalar_string(resolve_field("file.folder", &entry)).as_deref(),
            Some("/vault/Projects/Active"),
        );
        assert_eq!(scalar_string(resolve_field("file.name", &entry)).as_deref(), Some("Launch"));
    }

    #[test]
    fn file_name_falls_back_to_filename_stem_when_title_empty() {
        let entry = make_entry(|e| {
            e.path = "/vault/note.md".into();
            e.filename = "note.md".into();
        });
        assert_eq!(scalar_string(resolve_field("file.name", &entry)).as_deref(), Some("note"));
    }

    #[test]
    fn file_ext_returns_extension_without_dot() {
        let entry = make_entry(|e| e.filename = "doc.md".into());
        assert_eq!(scalar_string(resolve_field("file.ext", &entry)).as_deref(), Some("md"));
    }

    #[test]
    fn file_size_returns_string() {
        let entry = make_entry(|e| e.file_size = 4096);
        assert_eq!(scalar_string(resolve_field("file.size", &entry)).as_deref(), Some("4096"));
    }

    #[test]
    fn file_ctime_and_mtime_format_iso() {
        let entry = make_entry(|e| {
            e.created_at = Some(1_710_000_000_000);
            e.modified_at = Some(1_710_864_000_000);
        });
        let ctime = scalar_string(resolve_field("file.ctime", &entry)).unwrap();
        let mtime = scalar_string(resolve_field("file.mtime", &entry)).unwrap();
        assert!(ctime.starts_with("2024-03-"));
        assert!(mtime.starts_with("2024-03-"));
    }

    #[test]
    fn file_ctime_returns_none_when_unset() {
        let entry = make_entry(|_| {});
        assert!(scalar_string(resolve_field("file.ctime", &entry)).is_none());
    }

    #[test]
    fn file_tags_returns_belongs_to_relationship() {
        let entry = make_entry(|e| {
            e.belongs_to = vec!["[[urgent]]".into(), "[[Q2 Launch]]".into()];
        });
        let targets = relationship_targets(resolve_field("file.tags", &entry)).unwrap();
        assert_eq!(targets, &["[[urgent]]".to_string(), "[[Q2 Launch]]".to_string()]);
    }

    #[test]
    fn formula_namespace_resolves_to_empty_scalar() {
        let entry = make_entry(|_| {});
        assert!(scalar_string(resolve_field("formula.something", &entry)).is_none());
    }

    #[test]
    fn unknown_namespace_falls_through_to_note() {
        // A field with a dot but no recognized namespace prefix is treated as the literal
        // property name under the note namespace.
        let entry = make_entry(|e| {
            let mut props: HashMap<String, serde_json::Value> = HashMap::new();
            props.insert("my.weird.key".into(), serde_json::json!("yes"));
            e.properties = props;
        });
        assert_eq!(
            scalar_string(resolve_field("my.weird.key", &entry)).as_deref(),
            Some("yes"),
        );
    }

    #[test]
    fn boolean_field_alias_normalizes_namespaced_form() {
        assert_eq!(boolean_field_alias("archived"), Some("archived"));
        assert_eq!(boolean_field_alias("note.archived"), Some("archived"));
        assert_eq!(boolean_field_alias("favorite"), Some("favorite"));
        assert_eq!(boolean_field_alias("note.favorite"), Some("favorite"));
        assert_eq!(boolean_field_alias("other"), None);
    }
}
