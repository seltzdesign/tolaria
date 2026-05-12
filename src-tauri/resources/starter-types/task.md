---
type: Type
icon: check-circle
color: blue
sidebar label: Tasks
---

# Task

Tasks are units of work tracked alongside notes. Each task is a markdown file with `type: task` frontmatter.

## Fields

- **status** — free-form string. Defaults from the parent project's `statuses` list, or from `SUGGESTED_STATUSES` for standalone tasks.
- **priority** — optional, e.g. `P0` / `P1` / `P2` / `P3`.
- **due**, **start**, **completed** — ISO 8601 date (`2026-05-20`) or datetime with timezone offset (`2026-05-20T14:00:00+02:00`).
- **assignee** — list of wikilinks to people notes, or `@github-username` strings.
- **project** — wikilink to a project note. Absence means a standalone task.
- **blocked_by** — list of wikilinks to tasks that must complete first. Local-only in v1; not mirrored to GitHub Projects.
- **labels** — list of strings.
- **estimate** — number (story points or hours).

## Sync

When the parent project is bound to a GitHub Project, the sync engine writes additional `github_*` fields. Edit them only through the UI.

See [ADR 0115](../docs/adr/0115-tasks-and-projects-as-typed-notes.md) for the locked v1 schema.
