use super::views::*;
use super::VaultEntry;
use std::fs;

mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_entry(overrides: impl FnOnce(&mut VaultEntry)) -> VaultEntry {
        let mut entry = VaultEntry::default();
        overrides(&mut entry);
        entry
    }

    fn make_project_view(name: &str) -> ViewDefinition {
        ViewDefinition {
            name: name.to_string(),
            icon: None,
            color: None,
            order: None,
            sort: None,
            display: None,
            group_by: None,
            columns: Vec::new(),
            list_properties_display: Vec::new(),
            filters: FilterGroup::All(vec![FilterNode::Condition(FilterCondition {
                field: "type".to_string(),
                op: FilterOp::Equals,
                value: Some(serde_yaml::Value::String("Project".to_string())),
                regex: false,
            })]),
        }
    }

    #[test]
    fn test_parse_simple_view() {
        let yaml = r#"
name: Active Projects
icon: rocket
filters:
  all:
    - field: type
      op: equals
      value: Project
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(def.name, "Active Projects");
        assert_eq!(def.icon.as_deref(), Some("rocket"));
        assert!(def.list_properties_display.is_empty());
        match &def.filters {
            FilterGroup::All(nodes) => {
                assert_eq!(nodes.len(), 1);
                match &nodes[0] {
                    FilterNode::Condition(c) => {
                        assert_eq!(c.field, "type");
                    }
                    _ => panic!("Expected condition"),
                }
            }
            _ => panic!("Expected All group"),
        }
    }

    #[test]
    fn test_evaluate_equals() {
        let yaml = r#"
name: Projects
filters:
  all:
    - field: type
      op: equals
      value: Project
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let matching = make_entry(|e| e.is_a = Some("Project".to_string()));
        let non_matching = make_entry(|e| e.is_a = Some("Note".to_string()));
        let entries = vec![matching, non_matching];

        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_evaluate_contains_relationship() {
        let yaml = r#"
name: Related to Target
filters:
  all:
    - field: Related to
      op: contains
      value: "[[target]]"
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let mut rels = HashMap::new();
        rels.insert(
            "Related to".to_string(),
            vec!["[[target]]".to_string(), "[[other]]".to_string()],
        );
        let matching = make_entry(|e| e.relationships = rels);

        let non_matching = make_entry(|_| {});
        let entries = vec![matching, non_matching];

        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_evaluate_regex_on_scalar_field() {
        let yaml = r#"
name: Regex Title
filters:
  all:
    - field: title
      op: contains
      value: "^alpha\\s+project$"
      regex: true
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let matching = make_entry(|e| e.title = "Alpha Project".to_string());
        let case_matching = make_entry(|e| e.title = "alpha project".to_string());
        let non_matching = make_entry(|e| e.title = "Alpha Notes".to_string());
        let entries = vec![matching, case_matching, non_matching];

        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn test_evaluate_regex_on_relationship_field() {
        let yaml = r#"
name: Regex Relationship
filters:
  all:
    - field: Related to
      op: contains
      value: "monday-(112|113)|Monday #112"
      regex: true
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let mut alias_rels = HashMap::new();
        alias_rels.insert(
            "Related to".to_string(),
            vec!["[[monday-112|Monday #112]]".to_string()],
        );
        let alias_match = make_entry(|e| e.relationships = alias_rels);

        let mut stem_rels = HashMap::new();
        stem_rels.insert("Related to".to_string(), vec!["[[monday-113]]".to_string()]);
        let stem_match = make_entry(|e| e.relationships = stem_rels);

        let mut other_rels = HashMap::new();
        other_rels.insert(
            "Related to".to_string(),
            vec!["[[tuesday-200|Tuesday]]".to_string()],
        );
        let non_matching = make_entry(|e| e.relationships = other_rels);

        let entries = vec![alias_match, stem_match, non_matching];
        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn test_invalid_regex_matches_nothing() {
        let yaml = r#"
name: Broken Regex
filters:
  all:
    - field: title
      op: contains
      value: "("
      regex: true
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let entries = vec![
            make_entry(|e| e.title = "Alpha Project".to_string()),
            make_entry(|e| e.title = "Beta Project".to_string()),
        ];

        let result = evaluate_view(&def, &entries);
        assert!(result.is_empty());
    }

    #[test]
    fn test_evaluate_nested_and_or() {
        let yaml = r#"
name: Complex
filters:
  all:
    - field: type
      op: equals
      value: Project
    - any:
        - field: status
          op: equals
          value: Active
        - field: status
          op: equals
          value: Planning
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let active_project = make_entry(|e| {
            e.is_a = Some("Project".to_string());
            e.status = Some("Active".to_string());
        });
        let planning_project = make_entry(|e| {
            e.is_a = Some("Project".to_string());
            e.status = Some("Planning".to_string());
        });
        let done_project = make_entry(|e| {
            e.is_a = Some("Project".to_string());
            e.status = Some("Done".to_string());
        });
        let active_note = make_entry(|e| {
            e.is_a = Some("Note".to_string());
            e.status = Some("Active".to_string());
        });

        let entries = vec![active_project, planning_project, done_project, active_note];
        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn test_evaluate_is_empty() {
        let yaml_empty = r#"
name: No Status
filters:
  all:
    - field: status
      op: is_empty
"#;
        let yaml_not_empty = r#"
name: Has Status
filters:
  all:
    - field: status
      op: is_not_empty
"#;
        let def_empty: ViewDefinition = serde_yaml::from_str(yaml_empty).unwrap();
        let def_not_empty: ViewDefinition = serde_yaml::from_str(yaml_not_empty).unwrap();

        let with_status = make_entry(|e| e.status = Some("Active".to_string()));
        let without_status = make_entry(|_| {});
        let entries = vec![with_status, without_status];

        assert_eq!(evaluate_view(&def_empty, &entries), vec![1]);
        assert_eq!(evaluate_view(&def_not_empty, &entries), vec![0]);
    }

    #[test]
    fn test_scan_views_reads_yml_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let views_dir = dir.path().join("views");
        fs::create_dir_all(&views_dir).unwrap();

        let yaml_a = "name: Alpha\nfilters:\n  all:\n    - field: type\n      op: equals\n      value: Note\n";
        let yaml_b =
            "name: Beta\nfilters:\n  any:\n    - field: status\n      op: equals\n      value: Active\n";
        fs::write(views_dir.join("a-view.yml"), yaml_a).unwrap();
        fs::write(views_dir.join("b-view.yml"), yaml_b).unwrap();
        fs::write(views_dir.join("readme.txt"), "ignore me").unwrap();

        let views = scan_views(dir.path());
        assert_eq!(views.len(), 2);
        assert_eq!(views[0].filename, "a-view.yml");
        assert_eq!(views[0].definition.name, "Alpha");
        assert_eq!(views[1].filename, "b-view.yml");
        assert_eq!(views[1].definition.name, "Beta");
    }

    #[test]
    fn test_scan_views_sorts_by_persisted_order_then_filename() {
        let dir = tempfile::TempDir::new().unwrap();
        let views_dir = dir.path().join("views");
        fs::create_dir_all(&views_dir).unwrap();

        let alpha =
            "name: Alpha\norder: 20\nfilters:\n  all:\n    - field: type\n      op: equals\n      value: Note\n";
        let beta =
            "name: Beta\norder: 10\nfilters:\n  all:\n    - field: type\n      op: equals\n      value: Note\n";
        let gamma =
            "name: Gamma\nfilters:\n  all:\n    - field: type\n      op: equals\n      value: Note\n";
        fs::write(views_dir.join("alpha.yml"), alpha).unwrap();
        fs::write(views_dir.join("beta.yml"), beta).unwrap();
        fs::write(views_dir.join("gamma.yml"), gamma).unwrap();

        let views = scan_views(dir.path());

        assert_eq!(
            views
                .iter()
                .map(|view| (view.filename.as_str(), view.definition.order))
                .collect::<Vec<_>>(),
            vec![
                ("beta.yml", Some(10)),
                ("alpha.yml", Some(20)),
                ("gamma.yml", None),
            ]
        );
    }

    #[test]
    fn test_migrate_views_from_old_location() {
        let dir = tempfile::TempDir::new().unwrap();
        let old_dir = dir.path().join(".laputa").join("views");
        fs::create_dir_all(&old_dir).unwrap();

        let yaml = "name: Migrated\nfilters:\n  all:\n    - field: type\n      op: equals\n      value: Note\n";
        fs::write(old_dir.join("test.yml"), yaml).unwrap();

        let views = scan_views(dir.path());
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].definition.name, "Migrated");

        assert!(dir.path().join("views").join("test.yml").exists());
        assert!(!old_dir.join("test.yml").exists());
    }

    #[test]
    fn test_save_and_read_view() {
        let dir = tempfile::TempDir::new().unwrap();

        let mut def = make_project_view("Test View");
        def.icon = Some("star".to_string());
        def.sort = Some("modified:desc".to_string());
        def.list_properties_display = vec!["Priority".to_string(), "Owner".to_string()];

        save_view(dir.path(), "test.yml", &def).unwrap();

        let views = scan_views(dir.path());
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].definition.name, "Test View");
        assert_eq!(views[0].definition.icon.as_deref(), Some("star"));
        assert_eq!(
            views[0].definition.list_properties_display,
            vec!["Priority".to_string(), "Owner".to_string()]
        );

        delete_view(dir.path(), "test.yml").unwrap();
        let views = scan_views(dir.path());
        assert_eq!(views.len(), 0);
    }

    #[test]
    fn test_delete_view_treats_missing_file_as_deleted() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("views")).unwrap();

        delete_view(dir.path(), "missing.yml").unwrap();

        assert!(scan_views(dir.path()).is_empty());
    }

    #[test]
    fn test_delete_view_treats_missing_views_directory_as_deleted() {
        let dir = tempfile::TempDir::new().unwrap();

        delete_view(dir.path(), "missing.yml").unwrap();

        assert!(scan_views(dir.path()).is_empty());
    }

    #[test]
    fn test_save_and_read_view_with_emoji_icon() {
        let dir = tempfile::TempDir::new().unwrap();

        let mut def = make_project_view("Monday");
        def.icon = Some("🗂️".to_string());

        save_view(dir.path(), "monday.yml", &def).unwrap();

        let views = scan_views(dir.path());
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].definition.name, "Monday");
        assert_eq!(views[0].definition.icon.as_deref(), Some("🗂️"));
    }

    #[test]
    fn test_wikilink_stem_matching() {
        let yaml = r#"
name: Linked
filters:
  all:
    - field: Topics
      op: contains
      value: "[[target]]"
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let mut rels = HashMap::new();
        rels.insert("Topics".to_string(), vec!["[[target|Alias]]".to_string()]);
        let matching = make_entry(|e| e.relationships = rels);

        let mut rels2 = HashMap::new();
        rels2.insert("Topics".to_string(), vec!["[[other|Alias]]".to_string()]);
        let non_matching = make_entry(|e| e.relationships = rels2);

        let entries = vec![matching, non_matching];
        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_body_contains_filters_on_snippet() {
        let yaml = r#"
name: Body Search
filters:
  all:
    - field: body
      op: contains
      value: "quarterly"
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let matching = make_entry(|e| {
            e.title = "Match".to_string();
            e.snippet = "This is the quarterly review summary".to_string();
        });
        let non_matching = make_entry(|e| {
            e.title = "No match".to_string();
            e.snippet = "Daily standup notes".to_string();
        });
        let case_match = make_entry(|e| {
            e.title = "Case match".to_string();
            e.snippet = "QUARTERLY PLANNING session".to_string();
        });

        let entries = vec![matching, non_matching, case_match];
        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0, 2]);
    }

    #[test]
    fn test_body_not_contains() {
        let yaml = r#"
name: Body Exclude
filters:
  all:
    - field: body
      op: not_contains
      value: "draft"
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let final_note = make_entry(|e| {
            e.snippet = "Final version of the document".to_string();
        });
        let draft_note = make_entry(|e| {
            e.snippet = "This is a draft version".to_string();
        });

        let entries = vec![final_note, draft_note];
        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_body_combined_with_type_filter() {
        let yaml = r#"
name: Combined
filters:
  all:
    - field: type
      op: equals
      value: Note
    - field: body
      op: contains
      value: "important"
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let yes = make_entry(|e| {
            e.is_a = Some("Note".to_string());
            e.snippet = "This is important content".to_string();
        });
        let wrong_type = make_entry(|e| {
            e.is_a = Some("Project".to_string());
            e.snippet = "This is important content".to_string();
        });
        let no_match = make_entry(|e| {
            e.is_a = Some("Note".to_string());
            e.snippet = "Regular content".to_string();
        });

        let entries = vec![yes, wrong_type, no_match];
        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_legacy_view_round_trip_omits_new_fields() {
        let yaml = r#"name: Active Projects
icon: rocket
filters:
  all:
    - field: type
      op: equals
      value: Project
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();
        assert!(def.display.is_none());
        assert!(def.group_by.is_none());
        assert!(def.columns.is_empty());

        let serialized = serde_yaml::to_string(&def).unwrap();
        assert!(!serialized.contains("display"));
        assert!(!serialized.contains("groupBy"));
        assert!(!serialized.contains("columns"));
    }

    #[test]
    fn test_view_with_display_board_round_trips() {
        let yaml = r#"name: Tasks Board
display: board
groupBy:
  property: status
  direction: asc
columns:
  - status
  - priority
  - due
filters:
  all:
    - field: type
      op: equals
      value: task
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(def.display, Some(ViewDisplay::Board)));
        let group = def.group_by.as_ref().unwrap();
        assert_eq!(group.property, "status");
        assert!(matches!(group.direction, Some(SortDirection::Asc)));
        assert_eq!(def.columns, vec!["status", "priority", "due"]);

        let serialized = serde_yaml::to_string(&def).unwrap();
        let reparsed: ViewDefinition = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(reparsed.columns, def.columns);
        assert!(matches!(reparsed.display, Some(ViewDisplay::Board)));
    }

    #[test]
    fn test_unknown_display_value_fails_to_parse() {
        let yaml = r#"name: Bogus
display: invalidvalue
filters:
  all: []
"#;
        let err = serde_yaml::from_str::<ViewDefinition>(yaml).unwrap_err();
        assert!(err.to_string().contains("invalidvalue"));
    }

    #[test]
    fn test_group_by_without_direction_round_trips() {
        let yaml = r#"name: Tasks Grouped
display: table
groupBy:
  property: priority
filters:
  all: []
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();
        let group = def.group_by.as_ref().unwrap();
        assert_eq!(group.property, "priority");
        assert!(group.direction.is_none());
    }

    #[test]
    fn test_file_mtime_after_relative_date() {
        let yaml = r#"name: Recently Edited
filters:
  all:
    - field: file.mtime
      op: after
      value: 1 week ago
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        // 2 hours ago in epoch millis
        let recent = (chrono::Utc::now().timestamp_millis() - 2 * 60 * 60 * 1000) as u64;
        // 30 days ago in epoch millis
        let stale = (chrono::Utc::now().timestamp_millis() - 30 * 24 * 60 * 60 * 1000) as u64;

        let recent_entry = make_entry(|e| e.modified_at = Some(recent));
        let stale_entry = make_entry(|e| e.modified_at = Some(stale));

        let entries = vec![recent_entry, stale_entry];
        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_file_folder_equals() {
        let yaml = r#"name: Active Projects Folder
filters:
  all:
    - field: file.folder
      op: equals
      value: /vault/Projects/Active
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let in_folder = make_entry(|e| {
            e.path = "/vault/Projects/Active/launch.md".to_string();
            e.filename = "launch.md".to_string();
        });
        let other_folder = make_entry(|e| {
            e.path = "/vault/Archive/old.md".to_string();
            e.filename = "old.md".to_string();
        });

        let entries = vec![in_folder, other_folder];
        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_file_tags_contains_wikilink() {
        let yaml = r#"name: Urgent
filters:
  all:
    - field: file.tags
      op: contains
      value: "[[urgent]]"
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let tagged = make_entry(|e| {
            e.belongs_to = vec!["[[urgent]]".to_string(), "[[other]]".to_string()];
        });
        let untagged = make_entry(|e| {
            e.belongs_to = vec!["[[other]]".to_string()];
        });

        let entries = vec![tagged, untagged];
        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_namespaced_note_status_matches_bare_status() {
        let bare_yaml = r#"name: Done
filters:
  all:
    - field: status
      op: equals
      value: Done
"#;
        let ns_yaml = r#"name: Done NS
filters:
  all:
    - field: note.status
      op: equals
      value: Done
"#;
        let bare: ViewDefinition = serde_yaml::from_str(bare_yaml).unwrap();
        let ns: ViewDefinition = serde_yaml::from_str(ns_yaml).unwrap();

        let done = make_entry(|e| e.status = Some("Done".to_string()));
        let open = make_entry(|e| e.status = Some("Open".to_string()));
        let entries = vec![done, open];

        assert_eq!(evaluate_view(&bare, &entries), vec![0]);
        assert_eq!(evaluate_view(&ns, &entries), vec![0]);
    }

    #[test]
    fn test_formula_field_resolves_to_empty() {
        let yaml = r#"name: Formula
filters:
  all:
    - field: formula.derived
      op: is_not_empty
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let any_entry = make_entry(|_| {});
        let entries = vec![any_entry];
        let result = evaluate_view(&def, &entries);
        assert!(result.is_empty());
    }

    #[test]
    fn test_multi_view_file_yields_one_view_per_entry() {
        let dir = tempfile::tempdir().unwrap();
        let views_dir = dir.path().join("views");
        fs::create_dir_all(&views_dir).unwrap();

        let multi = r#"views:
  - name: Inbox
    filters:
      all:
        - field: type
          op: equals
          value: Note
  - name: Done
    display: list
    filters:
      all:
        - field: status
          op: equals
          value: Done
"#;
        fs::write(views_dir.join("tasks.yml"), multi).unwrap();

        let views = scan_views(dir.path());
        assert_eq!(views.len(), 2);

        let names: Vec<&str> = views.iter().map(|v| v.definition.name.as_str()).collect();
        assert!(names.contains(&"Inbox"));
        assert!(names.contains(&"Done"));

        for view in &views {
            assert!(matches!(view.shape, ViewFileShape::Multi));
            let (base, index) = split_view_filename(&view.filename);
            assert_eq!(base, "tasks.yml");
            assert!(index.is_some());
        }
    }

    #[test]
    fn test_single_and_multi_view_files_coexist_in_views_dir() {
        let dir = tempfile::tempdir().unwrap();
        let views_dir = dir.path().join("views");
        fs::create_dir_all(&views_dir).unwrap();

        let single = r#"name: Solo
filters:
  all:
    - field: type
      op: equals
      value: Note
"#;
        let multi = r#"views:
  - name: First
    filters:
      all: []
  - name: Second
    filters:
      all: []
"#;
        fs::write(views_dir.join("solo.yml"), single).unwrap();
        fs::write(views_dir.join("paired.yml"), multi).unwrap();

        let views = scan_views(dir.path());
        assert_eq!(views.len(), 3);

        let solo = views
            .iter()
            .find(|v| v.definition.name == "Solo")
            .expect("solo view present");
        assert!(matches!(solo.shape, ViewFileShape::Single));

        let multi_views: Vec<_> = views
            .iter()
            .filter(|v| matches!(v.shape, ViewFileShape::Multi))
            .collect();
        assert_eq!(multi_views.len(), 2);
    }

    #[test]
    fn test_save_view_rejects_multi_view_synthetic_filename() {
        let dir = tempfile::tempdir().unwrap();
        let definition = make_project_view("Anything");
        let err = save_view(dir.path(), "tasks.yml#0", &definition).unwrap_err();
        assert!(err.contains("multi-view"));
    }

    #[test]
    fn test_delete_view_rejects_multi_view_synthetic_filename() {
        let dir = tempfile::tempdir().unwrap();
        let err = delete_view(dir.path(), "tasks.yml#1").unwrap_err();
        assert!(err.contains("multi-view"));
    }

    #[test]
    fn test_split_view_filename_handles_single_and_multi() {
        assert_eq!(split_view_filename("inbox.yml"), ("inbox.yml", None));
        assert_eq!(split_view_filename("tasks.yml#0"), ("tasks.yml", Some(0)));
        assert_eq!(split_view_filename("tasks.yml#2"), ("tasks.yml", Some(2)));
        // Non-numeric suffix: treat as a literal filename
        assert_eq!(split_view_filename("weird#abc"), ("weird#abc", None));
    }

    #[test]
    fn test_body_is_empty() {
        let yaml = r#"
name: Empty Body
filters:
  all:
    - field: body
      op: is_empty
"#;
        let def: ViewDefinition = serde_yaml::from_str(yaml).unwrap();

        let empty = make_entry(|e| e.snippet = String::new());
        let has_content = make_entry(|e| e.snippet = "Some text here".to_string());

        let entries = vec![empty, has_content];
        let result = evaluate_view(&def, &entries);
        assert_eq!(result, vec![0]);
    }
}
