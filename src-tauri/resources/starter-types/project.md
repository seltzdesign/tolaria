---
type: Type
icon: folder-kanban
color: amber
sidebar label: Projects
---

# Project

Projects are containers for tasks. A project note has `type: project` frontmatter and can optionally bind to a GitHub Project for bidirectional sync.

## Fields

- **task_folder** — vault-relative path where this project's tasks live. Defaults to the project note's own folder.
- **statuses** — list of allowed status values for tasks in this project. Defaults to `["Not started", "In progress", "Done"]`. When bound to a GitHub Project, mirrors the GH Status field options.
- **terminal_statuses** — optional list of statuses that count as "complete" for dependency resolution. Defaults to `[Done]`.
- **default_view** — `board` / `table` / `timeline`. Default `board`.

## GitHub binding (optional)

Set these to bind the project to a GitHub Project for bidirectional sync. See [ADR 0116](../docs/adr/0116-github-projects-bridge-narrow-exception-to-0056.md).

- **github_project_url**, **github_project_node_id**
- **sync_enabled** — `true` to participate in scheduled sync.
- **sync_interval_minutes** — default `5`.
- **link_to_issues** — `true` creates real GitHub Issues; `false` (default) uses Project draft items.
- **github_issue_repo** — `owner/repo`, required iff `link_to_issues` is `true`.
- **status_field** — GitHub Project custom field mapped to local `status`. Default `Status`.
- **field_mappings** — additional local-key → GitHub-field-name mappings.

See [ADR 0115](../docs/adr/0115-tasks-and-projects-as-typed-notes.md) for the locked v1 schema.
