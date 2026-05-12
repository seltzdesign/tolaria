---
type: ADR
id: "0115"
title: "Tasks and projects as typed notes"
status: active
date: 2026-05-12
---

## Context

Tolaria is adding a task and project management feature so users can plan and track work alongside their notes instead of switching to Notion, Linear, or another tracker. The core question is whether tasks and projects are a new top-level data type with their own struct, persistence path, and cache machinery, or whether they reuse the existing note model.

[ADR 0002](0002-filesystem-source-of-truth.md) makes the filesystem the single source of truth and rejects parallel databases. [ADR 0025](0025-type-field-canonical.md) establishes `type:` as the canonical frontmatter field for note types and [ADR 0096](0096-root-created-type-documents.md) defines how user-customizable type documents live in the vault root. A parallel `Task` struct would duplicate everything `VaultEntry` already provides — wikilink discovery, search indexing, cache, watcher reactivity, AI access — and would either make tasks invisible to those subsystems or force every subsystem to learn a second model.

The bridge to GitHub Projects v2 (see [ADR 0116](0116-github-projects-bridge-narrow-exception-to-0056.md)) adds a small amount of sync-only frontmatter to bound tasks. That bridge is opt-in per project; standalone tasks have no bridge metadata at all.

## Decision

**A task is a `VaultEntry` with `type: task` in its frontmatter, and a project is a `VaultEntry` with `type: project`. Neither introduces a new data type, storage path, or cache.**

Specifically:

1. Tasks and projects ship as user-customizable type documents at vault root (`task.md`, `project.md`), loaded by the standard type document loader per [ADR 0096](0096-root-created-type-documents.md).
2. The task frontmatter schema is locked at v1 as below. Fields may be added later (forwards-compatible); renaming or removing fields requires a new ADR and migration.
   ```yaml
   ---
   type: task                        # required, literal "task"
   title: "Implement task drag-drop" # optional; falls back to H1 per ADR 0044
   status: "In progress"             # free-form string (see decision 4)
   priority: P1                      # optional, free string
   due: 2026-05-20                   # date OR datetime (see decision 8)
   # due: 2026-05-20T14:00:00+02:00  # ...with time + timezone offset
   start: 2026-05-15                 # date OR datetime
   completed: 2026-05-18             # date OR datetime
   assignee:                         # optional, list of wikilinks or @gh-usernames
     - "[[Armin]]"
   project: "[[My Cool Project]]"    # optional; absence == standalone task
   blocked_by:                       # optional; tasks that must complete before this one (see decision 4)
     - "[[Set up CI]]"
   labels: [bug, frontend]           # optional, list of strings
   estimate: 3                       # optional, number
   # Bridge-managed (only present on synced tasks; see ADR 0116, ADR 0119):
   github_project_node_id: PVT_kwHO...
   github_item_node_id: PVTI_lAHO...
   github_issue_url: https://github.com/...
   github_sync_status: synced
   github_last_synced: 2026-05-12T14:30:00Z
   github_remote_snapshot_hash: a1b2c3d4
   ---
   ```
3. The project frontmatter schema is locked at v1 as below. Projects always have `task_folder` and `statuses`; the `github_*` block is set only when bound to a GitHub Project.
   ```yaml
   ---
   type: project
   title: "My Cool Project"
   task_folder: "Projects/My Cool Project/tasks"  # default: same folder as the project note
   statuses: ["Not started", "In progress", Done] # default: SUGGESTED_STATUSES
   terminal_statuses: [Done]                       # optional; default: [Done] (or the last entry of `statuses` if no "Done")
   default_view: board
   # GitHub binding (optional):
   github_project_url: https://github.com/users/seltzdesign/projects/7
   github_project_node_id: PVT_kwHO...
   sync_enabled: true
   sync_interval_minutes: 5
   link_to_issues: false                          # if true, syncs create real Issues
   github_issue_repo: seltzdesign/some-repo       # required iff link_to_issues == true
   status_field: Status                           # GH custom field mapped to local `status`
   field_mappings:
     priority: Priority
     due: "End date"
     start: "Start date"
     estimate: Estimate
   ---
   ```
4. Task dependencies are modeled by `blocked_by`, a list of wikilinks to tasks that must complete first. The reverse direction (`blocks`) is **derived at query time** by inverse lookup, not stored — `blocked_by` is the canonical single source of truth. Dependencies are local-only in v1; they are not mirrored to GitHub Projects v2 (which has no native blocking relationship), and the sync engine treats `blocked_by` as a **local-only field** that does not participate in change detection (see [ADR 0119 §2](0119-three-way-diff-lww-for-projects-sync.md)). A v2 ADR may define a remote mapping (custom field, Issue task list, or sub-issues). Circular dependencies (A → B → A) are detected at save time and surface a non-blocking warning; the save still succeeds because external YAML edits can introduce cycles that need to be recoverable inside the app. A task is considered "blocked" when any entry in its `blocked_by` list resolves to a task whose `status` is not in the project's `terminal_statuses` set (see decision 3).
5. `status` remains a free-form string per Tolaria's existing convention (see `SUGGESTED_STATUSES` in [src/lib/statusStyles.ts](../../src/lib/statusStyles.ts)). No backend enum constraint. Allowed values for a bound project come from the project note's `statuses` array, which mirrors the GitHub Project's Status field options exactly.
6. Tasks without a `project` field are valid. They have no GitHub binding and never sync. The Tasks home view (P7) shows all tasks regardless of project membership.
7. Synced tasks are written into the bound project's `task_folder`. Existing tasks that the user manually moves into that folder are NOT auto-bound to the project; project membership is determined solely by the `project` wikilink.
8. Date fields (`due`, `start`, `completed`) accept either an ISO 8601 date (`YYYY-MM-DD`) or a full RFC 3339 datetime (`YYYY-MM-DDTHH:MM:SS±HH:MM` / `...Z`). Time is optional so users who only care about "due this day" stay terse, but the schema is calendar-sync-ready from v1. Timezone offset is required on datetime values written by the UI; hand-edited YAML without an offset is parsed as the system's local timezone at load time, normalized to an explicit offset on next save. Day-granularity filters (`before today`, `due this week` — see [ADR 0048](0048-relative-date-expressions-in-view-filters.md)) match both shapes by ignoring the time component, preserving back-compat with the existing date filter machinery in [src-tauri/src/vault/view_date_filters.rs](../../src-tauri/src/vault/view_date_filters.rs). Sync to GitHub Projects v2 drops the time component because GH Project date fields are date-only; the sync log emits a one-time warning per task on first lossy sync. The bridge-only `github_last_synced` field is always a full datetime — it is a sync internal, not user data.

## Alternatives considered

- **Tasks and projects as typed notes** (chosen): tasks inherit wikilink discovery, search, cache, watcher reactivity, AI tooling, and the type document customization model for free. The cost is a few extra frontmatter fields on bound tasks.
- **New top-level `Task` struct alongside `VaultEntry`**: cleaner type boundary in Rust, but every existing subsystem (search, wikilinks, cache, MCP, AI agents) would need a parallel branch, and tasks would be invisible to features built before they existed. Rejected.
- **Parallel `.tasks/` folder with custom format (e.g., a single SQLite or JSON index)**: faster aggregate queries, but breaks "your vault is just `.md` files" — tasks would be unreadable outside the app, contradicting [ADR 0002](0002-filesystem-source-of-truth.md). Rejected.
- **Use `is_a:` instead of `type:`**: was the initial proposal in the planning doc; conflicts with [ADR 0025](0025-type-field-canonical.md) which canonicalized `type:`. Rejected during ADR drafting.
- **Enforce a fixed status enum in the backend**: would prevent typos but breaks parity with GitHub Projects (which allows per-project custom Status options) and contradicts the existing free-form-string convention. Rejected.
- **Store dependencies in both directions (`blocked_by` and `blocks`)**: faster query for "what does this block", but introduces a consistency bug class where the two directions disagree after an external YAML edit. The single-canonical-direction model trades a small query cost for data integrity. Rejected.
- **Add sub-tasks (`parent_task`) to v1 alongside dependencies**: hierarchy and blocking are different concepts and the parent plan defers hierarchy to v2. Sub-tasks would add nested rendering complexity to board/table/timeline views and a "auto-complete parent when all children done" semantic decision that v1 doesn't have time for. Deferred to v2.
- **Sync dependencies to GitHub Projects v2 in v1**: GH Projects has no native blocking field. The candidates are (a) a custom text field with a serialized list, (b) GitHub Issue task lists, (c) the `subIssues` GraphQL connection on linked Issues. Each has different semantics and rate-limit implications. Picking one is a v2 ADR; v1 keeps dependencies local. Deferred.
- **Date-only fields (no times allowed)**: simpler parser, matches the existing date filter convention exactly. Rejected because it locks out future calendar sync (iCal / CalDAV / Google Calendar all require datetimes) and forces users with "due at 2pm" semantics to encode time in a separate field or the task body — both awkward.
- **Datetime-only fields (always require a time)**: removes the optional-time parser branch but forces users to invent meaningless times (`midnight`?) for tasks where only the day matters. Rejected — most tasks in practice are day-granularity.
- **Store local-naive datetimes without timezone offsets**: matches how humans type and easier to write by hand. Rejected because vaults sync across devices via git ([ADR 0002](0002-filesystem-source-of-truth.md)); a task with `due: 14:00` is ambiguous when the author is in Berlin and the reader is in San Francisco. Offsets in stored data are mandatory for vault portability.

## Consequences

- Tasks are wikilinkable, searchable, cacheable, and renderable in any markdown editor with no extra work.
- The `github_*` frontmatter fields are visible to users on synced tasks. The `github_` prefix groups them visually; a v2 ADR may move them into a nested `_github:` block if YAML readability degrades.
- The `task` and `project` type documents in the vault root are user-editable per [ADR 0096](0096-root-created-type-documents.md). Adding a custom property to a type document propagates to new notes of that type automatically.
- Standalone tasks (no `project`) work fully offline and have no GitHub dependency. The bridge is opt-in per project, not per app.
- AI agents can read, edit, and create tasks using normal filesystem tools without learning a new schema beyond the locked frontmatter shape.
- Dependency display is a downstream UI concern (P4 board, P5 table, P6 timeline). At minimum: blocked cards are visually muted with a "blocked by N" badge; the timeline view draws arrows between dependent task bars; a "Ready to start" filter (no unresolved `blocked_by`) is a natural saved view we can ship as a starter.
- Synced tasks lose their `blocked_by` semantics when viewed on github.com because v1 doesn't mirror dependencies. This is acceptable for a personal-use fork but is the most obvious source of v2 work; the sync log surfaces a one-line warning on first sync of a task with `blocked_by` set.
- Datetime support unlocks future iCal/CalDAV/Google Calendar sync without a schema migration — a calendar sync ADR would only need to add export logic, not change the data model. The `start` + `due` pair already maps cleanly to a calendar event's start/end window.
- Tasks with times-of-day sync to github.com as date-only and round-trip back as date-only. If a user edits a task on github.com (which can only set the date), the next pull writes a date-only value to the local frontmatter, dropping any time the user previously had. Documented in the sync log warning so the loss is never silent.
- Re-evaluate if: bridge frontmatter grows past ~10 fields and starts dominating the YAML block, or if user feedback shows the free-form status causes too many typos in unbound projects.
