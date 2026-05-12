# Plan 001 — Tasks in Tolaria (markdown-native + GitHub Projects bridge)

## Context

The user maintains a personal fork of Tolaria and loves how it leans on git for vault sync. They miss task & project management (the thing Notion does well that Tolaria doesn't) and noticed that GitHub Projects v2 — which they already use programmatically via `gh` + GraphQL for AI-assisted planning — would be a natural backend since the workflow already requires GitHub.

Two existing ADRs make this non-trivial:

- **[ADR 0002](../adr/0002-filesystem-source-of-truth.md)** — vault is the single source of truth, app never owns the data. Tasks living only in GitHub Projects (a cloud GraphQL DB) violates this.
- **[ADR 0056](../adr/0056-system-git-cli-auth-no-provider-oauth.md)** — Tolaria explicitly removed all GitHub OAuth, PAT storage, and GitHub API client code in favor of system git auth.

After discussion the user is shipping this in their fork (no upstream concerns) and chose:

- **Storage:** tasks are notes with a `task` type (existing `VaultEntry` schema, just typed)
- **Bridge:** bidirectional sync with GitHub Projects v2 (markdown is source, GitHub is mirror with live two-way edits)
- **Scope:** "full first cut" — multi-project, multiple views (board / table / timeline), custom fields

This preserves ADR 0002 (tasks are still `.md` on disk, work offline, survive GitHub API changes) and narrows the ADR 0056 reversal to an opt-in bridge that users have to enable explicitly. Even in a fork, this is the architecturally correct shape: tasks are linkable from notes via `[[wikilinks]]`, AI agents can read/edit them with normal file tools, and GitHub Projects becomes a rich viewing surface — not the system of record.

**Intended outcome:** Tolaria becomes a credible Notion alternative for the user's personal workflow: notes + tasks + projects in one vault, with GitHub Projects providing the polished board/table/timeline views and the cross-device editing surface (github.com / GitHub mobile).

---

## Architectural foundations

### Data model — tasks and projects as typed notes

Both concepts reuse the existing `VaultEntry` ([src-tauri/src/vault/entry.rs](../../src-tauri/src/vault/entry.rs)) and the type system from [ADR 0025](../adr/0025-type-field-canonical.md) / [ADR 0096](../adr/0096-root-created-type-documents.md). No new top-level abstraction.

**Task note frontmatter:**

```yaml
---
is_a: task
status: in-progress            # backlog | todo | in-progress | blocked | done | cancelled
priority: P1                   # P0 | P1 | P2 | P3
due: 2026-05-20
start: 2026-05-15              # optional, used by timeline view
assignee: ["[[Armin]]"]        # wikilinks to people notes, or @gh-usernames
project: "[[My Cool Project]]" # wikilink to a project note
labels: [bug, frontend]
estimate: 3                    # story points / hours, configurable
# Bridge-only fields (added when synced):
github_project_node_id: PVT_kwHO...
github_item_node_id: PVTI_lAHO...
github_issue_url: https://github.com/seltzdesign/foo/issues/42
github_sync_status: synced     # synced | local-only | conflicted | error
github_last_synced: 2026-05-12T14:30:00Z
github_remote_snapshot_hash: a1b2c3d4    # for 3-way conflict detection
---

# Implement task drag-drop

Free-form markdown body becomes the GitHub Project item's description / linked issue body.
```

**Project note frontmatter** (`is_a: project`):

```yaml
---
is_a: project
github_project_url: https://github.com/users/seltzdesign/projects/7
github_project_node_id: PVT_kwHO...
default_view: board
status_field: Status           # which GH custom field maps to local `status`
statuses: [Backlog, Todo, "In Progress", Done]
field_mappings:
  priority: Priority           # local frontmatter key → GH field name
  due: "End date"
  estimate: Estimate
sync_enabled: true
sync_interval_minutes: 5
---
```

The project note's body holds the project's README. Opening the project note in Tolaria renders the board/table/timeline view of tasks that have `project: [[this project]]`.

### Views — extend the existing engine

[ADR 0040](../adr/0040-custom-views-yml-filter-engine.md) already gives us a filter/sort view engine via `ViewDefinition` ([src-tauri/src/vault/views.rs](../../src-tauri/src/vault/views.rs)) — saved views live as `.yml` files in `.laputa/views/`, sync via git, and use a familiar `all`/`any` filter tree with operators (`equals`, `contains`, `before`, `after`, etc.). Tasks ride this same engine. We extend `ViewDefinition` with:

- **`display: list | table | board | timeline | cards`** — current implicit "list" stays the default
- **`group_by`** — required for board, optional for table (default group field for board = `status`)
- **`columns: [...]`** — for table view, ordered visible columns including `file.X` metadata fields
- **`file.X` field references** — extend the field resolver in `vault/views.rs` to accept `file.name`, `file.path`, `file.folder`, `file.mtime`, `file.ctime`, `file.ext`, `file.size`, `file.tags`, plus helper predicates `file.inFolder(...)` and `file.hasTag(...)`. Today the resolver only handles a fixed set of struct fields, then falls back to frontmatter and relationships. We're explicitly adopting Obsidian Bases' `note.X` / `file.X` / `formula.X` namespace convention (see "Learning from Obsidian Bases" below).

New view shapes built on this:

- **Board** — group-by single-select field (default `status`), card per task, drag between columns
- **Table** — task-shaped fields rendered as proper cells (status pill, date badge, assignee avatars)
- **Timeline** — horizontal axis = date, bars span `start` → `due`, swimlanes by assignee or status
- **Cards** *(stretch, not in v1)* — for note-like browsing of any typed collection, not just tasks

Views remain reusable: a "My P0s due this week" view is a regular `.yml` file with task-shaped filters, no parallel system. Reuses [src-tauri/src/vault/view_date_filters.rs](../../src-tauri/src/vault/view_date_filters.rs) for relative dates ([ADR 0048](../adr/0048-relative-date-expressions-in-view-filters.md)).

### Learning from Obsidian Bases

Obsidian shipped **Bases** — a saved-query/view system that's the closest precedent for what we're building. Worth studying because Obsidian is the gold standard for markdown-vault tooling, and a lot of our prospective users come from there. Key takeaways and our position on each:

| Obsidian Bases | Tolaria approach |
|---|---|
| Saved query lives as a `.base` YAML file in the vault | ✅ Already do this with `.laputa/views/*.yml`. Stays. |
| Three property namespaces: `note.X` (frontmatter), `file.X` (built-in metadata), `formula.X` (computed) | 🎯 Adopt the namespace. Today our resolver auto-falls-back; let's make namespaces explicit and add `file.X` metadata fields. |
| View types: `table`, `cards`, `list`, `map` (NO board/kanban natively) | 🚀 Differentiation. We ship `board` and `timeline` natively because tasks are first-class for us; Obsidian leaves it to plugins. |
| Multiple views per file: `views:` is an array, each view has its own `type` / `filters` / `sort` / `groupBy` | 🎯 Adopt. Current model is one-view-per-file. Extend `.yml` schema with optional `views:` array (back-compat: single-view files still parse). |
| Filter operators: `==`, `!=`, `>`, `<`, `>=`, `<=`, `&&`, `||`, `!` plus structured `and:`/`or:`/`not:` blocks | ➗ We already have structured `all:`/`any:` + named ops (`equals`, `before`, etc.). Keep our structured form for UI generation; add `and`/`or`/`not` aliases for Bases-style readability. |
| Rich formula DSL with dot-method chains: `[1,2,3].filter(value > 2).map(value * 2).sum()`, `date()`, `now()`, `today()`, duration arithmetic `now() - "1 week"`, `if(cond, a, b)`, `file.hasTag()`, `file.inFolder()` | 🪜 v2. The DSL is gorgeous but huge. v1 adds only what tasks need: `today()`, `now()`, duration arithmetic on dates, `file.hasTag()`, `file.inFolder()`, `contains()`. Full computed columns + summaries deferred. |
| Summary aggregations: `Average`, `Min`, `Max`, `Sum`, `Range`, `Median`, `Stddev`, `Earliest`, `Latest`, `Checked`/`Unchecked`, `Empty`/`Filled`, `Unique` | 🪜 v2. Useful for "total estimate of done tasks this sprint" but not blocking v1. |
| Embeddable: `![[my-view.base]]` or `![[my-view.base#ViewName]]` lets a saved view render inline in any note | 🎯 Adopt. Project notes use this instead of an auto-render hook: the project README contains `![[my-project.yml#board]]`. More flexible (multiple embeds per project: a board + an "Overdue" table). |
| `this` context object — references the embedding note when embedded, the base file otherwise | 🎯 Adopt for embedded views. Lets a project README embed `![[tasks.yml]]` filtered by `note.project == this.file.name`. |
| Performance gotchas called out: `file.backlinks` and `file.properties` are vault-wide scans | 📝 Note for the resolver. Our resolver should warn (or refuse) when a view filter would trigger a full-vault backlink walk per row. |

**The strategic move:** we are not just adding tasks. We are extending Tolaria's view engine into a Bases-class system, with tasks as the first concrete use case that requires `board` + `timeline` + GitHub Projects bridge. This framing matters because:

1. **The board view becomes reusable for any typed collection** — projects, contacts, books, recipes — not just tasks. Same for timeline.
2. **A Tolaria user fleeing Obsidian Bases sees their mental model preserved.** Same `note.X` / `file.X` namespacing, same `.yml`-in-vault philosophy, same embed pattern.
3. **The GitHub Projects bridge stays our differentiator.** Obsidian has nothing equivalent, and our user already has GitHub in their workflow.
4. **`.base` file format compatibility is a future option.** Not v1 — we keep `.yml` — but the closer we stay to Bases semantically, the easier a "read `.base` files too" milestone becomes later. That would make a Tolaria vault openable in Obsidian (or vice versa) as long as task-shaped notes use standard frontmatter.

### GitHub Projects bridge — architecture

```
┌─────────────────────┐   on-save     ┌──────────────────┐   GraphQL    ┌─────────────────┐
│  .md task in vault  │ ────────────▶ │  Sync engine     │ ───────────▶ │ GitHub Projects │
│  (source of truth)  │ ◀──────────── │  (Rust, Tauri)   │ ◀─────────── │  v2 (mirror)    │
└─────────────────────┘   on-pull     └──────────────────┘   GraphQL    └─────────────────┘
                                              │
                                              ▼
                                      ┌──────────────────┐
                                      │  snapshot.json   │  (per-project, for 3-way diff)
                                      │  (in cache dir)  │
                                      └──────────────────┘
```

**Three-way conflict detection:** for each synced task, the engine stores the last-known-remote snapshot. On sync, compare: `local` vs. `snapshot` (= local change?), `remote` vs. `snapshot` (= remote change?). Both changed = conflict. Resolution policy v1: last-write-wins by `github_last_synced` + audit entry in `.tolaria/sync-log.jsonl`. Manual resolution UI is v2.

**Auth:** PAT (fine-grained, scoped to `repo` + `project`) stored in **OS keychain** via the `keyring` Rust crate — NOT `settings.json`. This is the narrow ADR 0056 exception we'll document in a new ADR. PAT entry happens once via Settings; the token never appears in the vault, settings file, or telemetry.

**Sync triggers:**
- On task save (debounced 2s, like the existing autosave from [ADR 0015](../adr/0015-auto-save-with-debounce.md))
- On app focus
- Periodic background poll (configurable, default 5 min)
- Manual "Sync now" command

**Rate limit handling:** primary GraphQL rate limit is 5000 points/hour. Each item-level mutation is ~1 point. Sync batches all changes into a single `updateProjectV2ItemFieldValue` mutation per item per cycle. Honor `X-RateLimit-Remaining` headers — back off when <100 remain.

### Drafts vs. issues

GitHub Projects v2 items can be **draft items** (project-only, no issue) or **linked issues**. Per-project config: `link_to_issues: bool`. When true, syncing a `.md` task creates a real GitHub Issue in a configured repo (`github_issue_repo: owner/repo`). Default: draft items only (no issue creation), keeps it simple and avoids polluting issue trackers.

---

## Implementation phases

Each phase below is sized for ~1–3 days of focused work and is independently shippable / QA-able per the Todoist task workflow in [AGENTS.md](../../AGENTS.md). Phases follow strict dependency order (later phases assume earlier ones are merged).

The phases group into five tracks:

- **Foundations** (P0–P2) — schema and core editing
- **Views** (P3–P7) — view engine extension, board, table, timeline, embedded views & home
- **Bridge plumbing** (P8–P10) — auth, GraphQL client, binding
- **Bridge sync** (P11–P16) — pull, push, conflicts, scheduler, UI, offline
- **Release** (P17–P19) — l10n, QA, docs

### Track A — Foundations

#### P0 · ADRs and schema lock-in (~1–2 days)

Write five new ADRs (numbers will be next available, currently `0101+`):

1. **"Tasks as typed notes, not a parallel data type"** — extends [ADR 0025](../adr/0025-type-field-canonical.md); explains why tasks are `VaultEntry` with `is_a: task`, not a new struct.
2. **"GitHub Projects v2 bridge — narrow exception to ADR 0056"** — re-introduces PAT storage *in OS keychain only*, scoped exclusively to Projects sync, opt-in per project.
3. **"View engine extension: display modes, group-by, `file.X` fields, multi-view files"** — supersedes parts of [ADR 0040](../adr/0040-custom-views-yml-filter-engine.md). Documents the namespace adoption (`note.X` / `file.X` / `formula.X`), new display modes (`table`/`board`/`timeline`/`cards`), `group_by`, and the optional `views:` array for multiple views per `.yml` file. Calls out Obsidian Bases as inspiration and the `.base` compatibility door for v2+.
4. **"Embeddable view files in notes"** — `![[view.yml]]` and `![[view.yml#name]]` render inline; defines `this` context for filters in embedded views.
5. **"Three-way diff with last-write-wins for Projects sync conflicts"** — documents conflict policy and the per-project snapshot file.

**Acceptance:** five ADRs merged. Frontmatter schemas for `task` and `project`, and the extended `ViewDefinition` schema, documented in the ADRs verbatim — locked from this point forward. Explicit list of v1 filter functions (`today()`, `now()`, duration arithmetic, `file.hasTag()`, `file.inFolder()`, `contains()`) — anything else is v2.

#### P1 · Task & project types + frontmatter validation (~2 days)

**Backend:**
- Register `task` and `project` as well-known types in [src-tauri/src/vault/entry.rs](../../src-tauri/src/vault/entry.rs) (typed helpers `as_task()`, `as_project()`)
- Extend [src-tauri/src/vault/frontmatter.rs](../../src-tauri/src/vault/frontmatter.rs) with task-specific coercion (status enum, priority enum, ISO date parsing for `due`/`start`)
- Add starter type documents at `src-tauri/resources/starter-types/{task,project}.md` ([ADR 0096](../adr/0096-root-created-type-documents.md))
- New Tauri command `create_task(folder, title, project)` in new `src-tauri/src/commands/tasks.rs`
- Register in [src-tauri/src/commands/mod.rs](../../src-tauri/src/commands/mod.rs)

**Acceptance:** can create a task via Tauri command from devtools and it appears as a valid `.md` file with correct frontmatter. Rust tests cover status/priority/date coercion. CodeScene 10.0 on new files.

#### P2 · Task editor UI (~2 days)

**Frontend:**
- New `src/components/tasks/TaskEditor.tsx` — wraps the existing note editor, adds a task-property side panel (status pill, priority dropdown, due/start date pickers using shadcn `Calendar`+`Popover`, assignee combobox reusing the wikilink autocomplete, project picker)
- New `src/hooks/useTasks.ts` — list / create / update wrappers around the Tauri commands
- When a note's `is_a` is `task`, render `TaskEditor` instead of the standard editor
- All inputs from shadcn/ui per [AGENTS.md §3](../../AGENTS.md#ui-components--mandatory-rules) — no raw HTML

**Acceptance:** open a task `.md` → property panel renders; change status/priority/due → frontmatter on disk updates within debounce window. Playwright test creates a task and edits each property.

### Track B — Views

#### P3 · View engine extensions: `file.X` fields, display modes, multi-view files (~2–3 days)

Backend-only phase. Lays the groundwork for every Track B UI phase that follows. No new UI in this phase — the existing list view keeps working unchanged.

- Extend `ViewDefinition` in [src-tauri/src/vault/views.rs](../../src-tauri/src/vault/views.rs):
  - Add `display: list | table | board | timeline | cards` (default `list` for back-compat)
  - Add `group_by: { property, direction }` (used by board, optional for others)
  - Add `columns: [String]` for table-mode column order
- **Extend the field resolver** to support the `note.X` / `file.X` / `formula.X` namespaces (per the "Learning from Obsidian Bases" section):
  - `note.<name>` → frontmatter property (existing behavior, made explicit)
  - `file.name`, `file.path`, `file.folder`, `file.ext`, `file.size`, `file.ctime`, `file.mtime`, `file.tags` — pull from `VaultEntry` / filesystem metadata
  - Bare names (e.g. `status`) keep resolving to `note.status` for back-compat
  - `formula.X` reserved but unimplemented (errors with "v2 feature" message)
- **Extend the filter operator set** with v1 helper functions: `today()`, `now()`, duration arithmetic (`now() - "1 week"`), `file.hasTag(...)`, `file.inFolder(...)`, `contains(haystack, needle)`. Nothing else.
- **Multi-view file format:** allow optional top-level `views:` array in a `.yml` view file (each entry is a full `ViewDefinition` body minus the file-level metadata). Single-view files keep parsing as-is — back-compat is mandatory because users have existing views.
- New `src-tauri/src/vault/view_migration.rs` change: detect old single-view format vs. new multi-view format; serialize back in same shape it was loaded.

**Acceptance:** existing views in `.laputa/views/` continue to render exactly as before. New unit tests cover (a) `file.mtime > now() - "1 week"` filtering, (b) `file.hasTag("urgent")` predicate, (c) `file.inFolder("Projects/Active")` predicate, (d) round-tripping a multi-view `.yml` file. CodeScene 10.0 on touched/new files.

#### P4 · Board view + drag-drop (~2 days)

- New `src/components/tasks/TaskBoard.tsx` — kanban columns derived from the bound project's `statuses` list (or from distinct values of the `group_by` field if no project binding), cards drag between columns via `@dnd-kit/core` (check if already a dep; add if not)
- Dropping a card writes the new value of the `group_by` field to the task's frontmatter via the existing update path — same code path as inline edits
- New `src/components/tasks/TaskCard.tsx` — shared card primitive (title, priority chip, due badge, assignee avatars); reused later by timeline view
- Wire up `display: board` → renders `TaskBoard` in the view container

**Acceptance:** create a saved view with `display: board, group_by: status` → board renders columns from observed status values, cards group correctly, drag updates the file. Playwright drag test in regression lane (not smoke — drag is brittle).

#### P5 · Table view (~1–2 days)

- New `src/components/tasks/TaskTable.tsx` — dense table driven by the `columns` array; inline-editable status pill, date, priority, assignee
- Sort by any column header click (writes `sort` back to the `.yml` per existing ADR 0040 behavior), sticky header
- Reuse existing table primitives if any exist in `src/components/` — search first
- Wire up `display: table` → renders `TaskTable`

**Acceptance:** create a saved view with `display: table, columns: [title, status, due, assignee, file.mtime]` → renders correctly including the `file.mtime` column. Click a sort header → `.yml` updates. Inline status change writes to disk.

#### P6 · Timeline view (~3 days)

- New `src/components/tasks/TaskTimeline.tsx` — horizontal swimlane gantt, x-axis = date, y-axis = swimlane (`group_by` field, default `assignee`)
- Hand-rolled SVG (avoid heavy `vis-timeline` dep); virtualize if >500 tasks
- Bars span `start` → `due`; drag-resize updates dates via the same save path
- Wire up `display: timeline` → renders `TaskTimeline`

**Acceptance:** project with 20 tasks across 4 weeks renders correctly; resizing a bar persists new dates to disk.

#### P7 · Embedded views in notes + Tasks home (~2–3 days)

Implements the [ADR for embeddable view files](../adr/0102-embedded-view-files.md) (number TBD; see P0). Replaces the original "project note auto-render" idea with a more flexible embed model — same outcome for project READMEs, but composable.

- New transclusion handler: when an editor encounters `![[view.yml]]` or `![[view.yml#viewname]]`, render the named view inline. Use the same component dispatch as full-tab view rendering.
- Implement the `this` context object for embedded views: filters can reference `this.file.name`, `this.file.path`, etc. — resolves to the embedding note. Lets a project note embed a view filtered by `note.project == this.file.name`.
- New "Tasks" entry in the sidebar rail — opens a default aggregate view "All open tasks across projects" (a regular saved view shipped as a starter file in `.laputa/views/`).
- New `src/hooks/useProject.ts` — small utility for the common "tasks where `project == this`" pattern, used by project note templates.
- Update the `project` starter type document to include `![[project-board.yml]]` in its body so new project notes automatically embed a board.

**Acceptance:** create a project note → starter template embeds a board scoped to that project's tasks. Edit a task from the embedded board → frontmatter updates. Sidebar "Tasks" opens the all-tasks view. Multiple embeds in one note work (e.g., board + overdue table side-by-side).

### Track C — Bridge plumbing

#### P8 · GitHub PAT storage + Settings UI (~2 days)

- New dep: `keyring = "3"` (cross-platform OS keychain)
- New `src-tauri/src/github/projects/auth.rs` — `store_pat()`, `load_pat()`, `delete_pat()` under service `"com.tolaria.app.github_pat"`
- Extend [src-tauri/src/settings.rs](../../src-tauri/src/settings.rs) with **non-secret** config only (`github_projects_enabled: bool`, `github_default_sync_interval_minutes: u32`). PAT itself never goes in `settings.json`.
- New "GitHub Projects" section in [src/components/SettingsPanel.tsx](../../src/components/SettingsPanel.tsx) — write-only PAT input, "Test connection" button (calls a minimal `viewer { login }` query), "Clear credentials"
- New l10n keys for the section (deferred to P17 for full translation)

**Acceptance:** enter PAT → stored in OS keychain (verify on macOS via `security find-generic-password`, Windows via Credential Manager). "Test connection" returns the GitHub username. Clear credentials removes the entry.

#### P9 · GitHub Projects GraphQL client (~3 days)

- New deps: `reqwest = { version = "0.12", features = ["json", "rustls-tls"] }`, `graphql_client = "0.14"`
- New module `src-tauri/src/github/projects/client.rs` with typed queries:
  - `ListProjectsForUser` / `ListProjectsForOrg`
  - `GetProjectFields` (returns the project's custom field schema)
  - `ListProjectItems` (cursor-paginated, 100/page)
  - `AddDraftIssue`, `UpdateProjectItemField`, `DeleteProjectItem`
  - `LinkIssueToProject` (gated behind `link_to_issues`)
- New `src-tauri/src/github/projects/rate_limit.rs` — parse `X-RateLimit-Remaining`, exponential backoff when <100 remain
- Reuse the existing tokio runtime; do not spin up a second one

**Acceptance:** Rust integration test against a fixture project (or recorded via `wiremock`) covers each query/mutation. Rate-limit unit tests cover backoff math.

#### P10 · Bind project modal + field mapping (~2 days)

- New `src/components/tasks/BindGitHubProjectModal.tsx` — opens from a project note's "·" menu
- Flow: paste Project URL → parse `org|user` + project number → `GetProjectFields` → field-mapping UI (local key → GH field) with smart defaults (`status` → field named "Status", `due` → "Due" or "End date")
- New Tauri command `bind_github_project(note_path, project_url, field_mapping)` in new `src-tauri/src/commands/github_projects.rs` — writes the binding into the project note's frontmatter, runs an initial discovery pull (no item sync yet — that's P11)

**Acceptance:** bind a real personal GitHub Project; field-mapping UI lists actual GH fields; binding persists in the project note's frontmatter; "Unbind" reverses it cleanly.

### Track D — Bridge sync

#### P11 · Pull sync + snapshot store (~3 days)

- New `src-tauri/src/github/projects/snapshot.rs` — per-project snapshot at `<cache-dir>/github-sync/<project_node_id>.json` ([ADR 0024](../adr/0024-cache-outside-vault.md))
- New `src-tauri/src/github/projects/sync.rs` with `pull(project)`:
  - `ListProjectItems` (paginated)
  - Diff each remote item against snapshot
  - Create/update/delete `.md` task files in the vault folder configured for this project
  - Honor [ADR 0077](../adr/0077-concurrent-safe-vault-cache-replacement.md) and [ADR 0075](../adr/0075-crash-safe-note-rename-transactions.md) for crash safety
  - Update snapshot at end of cycle
- New Tauri command `github_sync_pull(project_node_id)` for manual pull
- Sync log appended to `.tolaria/sync-log.jsonl` (in vault, gitignored)

**Acceptance:** bind a project with 10 items → pull creates 10 `.md` files with correct frontmatter. Re-run pull → no changes. Modify item on github.com → re-run pull → local file updates. Delete item on github.com → re-run pull → local file deleted.

#### P12 · Push sync (~2 days)

- Extend `src-tauri/src/github/projects/sync.rs` with `push(project, local_changes)`:
  - On task save (debounced), detect which frontmatter fields changed vs. snapshot
  - Run `UpdateProjectItemField` for each changed field (batched per item)
  - For brand-new local tasks: `AddDraftIssue` to create the item, then field updates
  - Update snapshot + `github_last_synced` on success
- Hook into the existing autosave path ([ADR 0015](../adr/0015-auto-save-with-debounce.md)) so pushes happen ~2s after edits stop

**Acceptance:** edit task locally → after debounce, change reflects on github.com. Create new task locally → appears as draft item on github.com. Push errors surface to user (don't silently fail).

#### P13 · Conflict detection + LWW resolution (~2 days)

- Extend sync engine with `reconcile(project)`:
  - For each item, compare local vs. snapshot (local change?), remote vs. snapshot (remote change?)
  - Both changed → conflict
  - LWW by newer of `github_last_synced` vs. file mtime
  - **Loser written to `<cache-dir>/github-sync/conflicts/<task_id>.<timestamp>.md`** — recoverable
  - Audit entry to sync log with both versions' hashes
- Set `github_sync_status: conflicted` on the local task temporarily (cleared once resolved)

**Acceptance:** disconnect → edit same task locally and on github.com → reconnect → LWW chosen, loser preserved in cache dir, audit log entry written.

#### P14 · Background scheduler + reactive sync events (~2 days)

- New `src-tauri/src/github/projects/scheduler.rs` — Tokio task spawned at app start for each project with `sync_enabled: true`
- Honors per-project `sync_interval_minutes`
- Cancellable + restartable on settings change
- Emits Tauri events: `task_synced`, `task_conflict`, `sync_started`, `sync_finished`, `sync_error`
- Sync triggers: app focus, periodic interval, manual command, post-save (debounced via P12)

**Acceptance:** with two bound projects, scheduler runs both on their configured intervals. Toggling `sync_enabled` off cancels the task. Listing Tauri events in devtools shows the lifecycle events.

#### P15 · Sync status UI indicators (~1 day)

- New `src/components/tasks/SyncStatusIndicator.tsx` — small icon in project header (green / yellow / red / grey based on aggregate state), tooltip shows last-sync time
- Per-task badge on cards/rows (synced / local-only / conflicted / error)
- Click indicator → opens "Sync log" drawer
- New `src/hooks/useGitHubProjectSync.ts` — subscribes to Tauri sync events ([ADR 0043](../adr/0043-reactive-vault-state-on-save.md) pattern)

**Acceptance:** edit a task offline → yellow badge. Reconnect + sync → green. Force a conflict → red badge with tooltip explanation.

#### P16 · Offline queue + network awareness (~2 days)

- Per [ADR 0060](../adr/0060-network-aware-ui-gating-for-remote-features.md), detect online state
- When offline, sync engine queues pending push deltas in `<cache-dir>/github-sync/pending/<project_node_id>.jsonl`
- On reconnect, drain queue (FIFO) with normal conflict checking
- UI surfaces "Offline — N changes queued" indicator

**Acceptance:** disconnect → make 5 task edits → reconnect → queue drains within one sync cycle, all 5 changes appear on github.com.

### Track E — Release

#### P17 · L10n + PostHog telemetry (~2 days)

- Add all new UI keys to [src/lib/locales/en.json](../../src/lib/locales/en.json) (~80–120 keys: statuses, view names, sync states, settings labels, error messages)
- Run `pnpm l10n:translate` to populate all bundled locales, then `pnpm l10n:validate`
- Add PostHog events in [src/lib/telemetry.ts](../../src/lib/telemetry.ts):
  - `task_created`, `task_status_changed`, `task_completed`
  - `project_created`, `project_bound_to_github`, `project_unbound`
  - `github_sync_started`, `github_sync_completed { items_pushed, items_pulled, conflicts }`, `github_sync_error { error_type }`
  - `task_view_switched { from, to }`, `view_embedded_in_note`
- **No PII, no task titles in payloads** ([AGENTS.md §2 PostHog rules](../../AGENTS.md#product-analytics-mandatory-for-meaningful-features))

**Acceptance:** `pnpm l10n:validate` clean for all bundled locales. PostHog event names appear in dev console when actions are taken.

#### P18 · Playwright smoke + coverage + Codacy + CodeScene (~3 days)

- New `tests/smoke/tasks.spec.ts` — tagged `@smoke` only for core "create task → save → reload" flow (per [AGENTS.md §1c smoke rules](../../AGENTS.md#1c-when-done)). Drag, view-switching, sync go in regression lane.
- Targeted unit tests bringing frontend coverage to ≥70% (`pnpm test:coverage`) and Rust line coverage to ≥85% (`cargo llvm-cov --fail-under-lines 85`) — focus on `sync.rs`, `client.rs`, `frontmatter.rs` task schema, `views.rs` extended resolver
- Codacy: `.codacy/cli.sh analyze` on every touched file; fix all new Critical/High
- CodeScene: every new file at 10.0, every touched file improved or held. Update `.codescene-thresholds` ratchet if Hotspot/Average improved.

**Acceptance:** `pnpm playwright:smoke` < 5 min, coverage gates pass, Codacy clean, CodeScene Hotspot + Average ≥ `.codescene-thresholds`.

#### P19 · Docs update + final QA + release (~1 day)

- Update [docs/ARCHITECTURE.md](../ARCHITECTURE.md) with the extended view engine and bridge architecture diagram + sync flow
- Update [docs/ABSTRACTIONS.md](../ABSTRACTIONS.md) with task/project type docs, the `note.X`/`file.X`/`formula.X` namespace, and sync engine entry
- Update `docs/GETTING-STARTED.md` with "Tasks & projects" walkthrough (create project → embed a board → bind to GitHub → first sync)
- Native QA pass per [AGENTS.md §1c Phase 2](../../AGENTS.md#1c-when-done): `pnpm tauri dev`, screenshot, smoke through all 19 phases' features
- Demo vault hygiene: `git status --short -- demo-vault demo-vault-v2` clean
- Tag and release per the channel rules in [ADR 0066](../adr/0066-calendar-semver-versioning-for-alpha-and-stable-releases.md)

**Acceptance:** docs land in same commit as final code, native QA screenshots attached to Todoist completion comment, demo vault clean, release tagged.

---

## Critical files to modify or create

| Area | Path | Action |
|---|---|---|
| Type schema | [src-tauri/src/vault/entry.rs](../../src-tauri/src/vault/entry.rs) | Extend with task/project helpers (P1) |
| Schema validation | [src-tauri/src/vault/frontmatter.rs](../../src-tauri/src/vault/frontmatter.rs) | Add task/project property coercion (P1) |
| View engine | [src-tauri/src/vault/views.rs](../../src-tauri/src/vault/views.rs) | Add display modes, group_by, `note.X`/`file.X` namespace, helper fns (P3); board/table/timeline wired in P4–P6 |
| View migration | [src-tauri/src/vault/view_migration.rs](../../src-tauri/src/vault/view_migration.rs) | Multi-view file format (P3) |
| Settings | [src-tauri/src/settings.rs](../../src-tauri/src/settings.rs) | Add non-secret bridge config (P8) |
| Settings UI | [src/components/SettingsPanel.tsx](../../src/components/SettingsPanel.tsx) | Add GitHub Projects section (P8) |
| Tauri commands | [src-tauri/src/commands/mod.rs](../../src-tauri/src/commands/mod.rs) | Register `tasks` + `github_projects` modules (P1, P10) |
| Task commands | `src-tauri/src/commands/tasks.rs` | New (P1) |
| Bridge commands | `src-tauri/src/commands/github_projects.rs` | New (P10) |
| GH client | `src-tauri/src/github/projects/{mod,client,auth,sync,scheduler,snapshot,rate_limit}.rs` | New module (P8–P16) |
| Type templates | `src-tauri/resources/starter-types/{task,project}.md` | New (P1); project template gets embedded view in P7 |
| Task editor | `src/components/tasks/TaskEditor.tsx` | New (P2) |
| Board UI | `src/components/tasks/TaskBoard.tsx` | New (P4) |
| Card primitive | `src/components/tasks/TaskCard.tsx` | New (P4) |
| Table UI | `src/components/tasks/TaskTable.tsx` | New (P5) |
| Timeline UI | `src/components/tasks/TaskTimeline.tsx` | New (P6) |
| Embed handler | editor transclusion path (find in `src/components/editor/`) | Extend for `![[view.yml]]` (P7) |
| Bind modal | `src/components/tasks/BindGitHubProjectModal.tsx` | New (P10) |
| Sync indicator | `src/components/tasks/SyncStatusIndicator.tsx` | New (P15) |
| Hooks | `src/hooks/{useTasks,useProject,useGitHubProjectSync}.ts` | New (P2, P7, P15) |
| L10n | [src/lib/locales/en.json](../../src/lib/locales/en.json) | Add ~100 keys, run `pnpm l10n:translate` (P17) |
| Telemetry | [src/lib/telemetry.ts](../../src/lib/telemetry.ts) | Add event constants (P17) |
| ADRs | `docs/adr/0101-...0105-*.md` | New (P0) — five ADRs |

## Reusable existing utilities

- **VaultEntry / frontmatter parsing** — [src-tauri/src/vault/entry.rs](../../src-tauri/src/vault/entry.rs), [src-tauri/src/vault/frontmatter.rs](../../src-tauri/src/vault/frontmatter.rs). Don't invent a Task struct; tasks ARE VaultEntries.
- **View engine** — [src-tauri/src/vault/views.rs](../../src-tauri/src/vault/views.rs) and view date filters at [src-tauri/src/vault/view_date_filters.rs](../../src-tauri/src/vault/view_date_filters.rs). Extend, don't fork.
- **Wikilink autocomplete** — existing component in `src/components/` (assignee + project fields). Search before reimplementing.
- **Emoji / color pickers** — reuse for task labels and project icons.
- **Save debounce** — pattern from [ADR 0015](../adr/0015-auto-save-with-debounce.md); apply to sync trigger.
- **Reactive vault state** — pattern from [ADR 0043](../adr/0043-reactive-vault-state-on-save.md) and the filesystem watcher in [ADR 0089](../adr/0089-active-vault-filesystem-watcher.md).
- **Cache outside vault** — per [ADR 0024](../adr/0024-cache-outside-vault.md), sync snapshots and conflict copies live in the cache dir.
- **Network gating** — per [ADR 0060](../adr/0060-network-aware-ui-gating-for-remote-features.md).

## Final verification — end-to-end

After P19, run this checklist before declaring the feature done:

1. **Cold-start markdown-only flow** (no GitHub): project + 10 tasks → board/table/timeline → close app → reopen → state persists in `.md` files.
2. **Embedded views:** project README contains `![[my-project-board.yml]]` → board renders inline scoped via `this.file.name`.
3. **Bind & initial pull:** Settings → PAT → Test connection → bind project → initial pull populates tasks.
4. **Bidirectional sync:** edit locally → appears on github.com within sync interval. Edit on github.com → appears in Tolaria.
5. **Conflict handling:** offline edit on both sides → reconnect → LWW + recoverable conflict copy in cache.
6. **Offline graceful degradation:** disconnect → 5 edits queue → reconnect → drain.
7. **Wikilink integration:** `[[link to a task]]` from a note resolves; back-link appears on the task.
8. **Type document customization:** edit the `task` type doc → add custom field → new tasks pick it up → bind UI shows it.
9. **File-metadata filters:** view with `file.mtime > now() - "1 week"` correctly lists recently-modified tasks.
10. **Release gates:** lint, tsc, vitest, playwright:smoke, cargo test, cargo llvm-cov, CodeScene, Codacy, l10n:validate, demo-vault clean.

## Out of scope for v1

- **Manual conflict resolution UI** — v2. v1 is LWW + audit + recoverable conflict copies.
- **Sub-tasks / task hierarchies** — possible via existing wikilink relationships, but no dedicated UI in v1.
- **Recurring tasks** — v2.
- **GitHub Issues full lifecycle** (creating new repos, closing PRs) — out of scope. Only the Projects v2 surface.
- **GitLab / Jira / Linear bridges** — out of scope. GitHub Projects only in v1.
- **Mobile (iOS) parity for sync** — Phase A markdown task system works on iOS per [ADR 0005](../adr/0005-tauri-ios-for-ipad.md); the sync scheduler is desktop-first for v1.
- **Time tracking** — out of scope.

## Estimated effort

Roughly **6–8 weeks** of focused work across 19 phases:

| Track | Phases | Days |
|---|---|---|
| A — Foundations | P0–P2 | ~5 |
| B — Views | P3–P7 | ~12 |
| C — Bridge plumbing | P8–P10 | ~7 |
| D — Bridge sync | P11–P16 | ~12 |
| E — Release | P17–P19 | ~6 |
| **Total** | **19 phases** | **~42 days** |

Each phase maps cleanly to a single Todoist task per [AGENTS.md §1](../../AGENTS.md#1-task-workflow), with its own completion comment, CodeScene before/after, QA artifacts, and release-readiness check.

The ~3-day uplift vs. the previous 18-phase version is the new **P3** (view engine extensions) and the expanded **P7** (embedded views with `this` context) — both directly justified by the Bases compatibility lessons. P3 is a strict prerequisite for all subsequent Track B UI phases, so the cost is unavoidable; P7's expansion replaces what was a narrow "auto-render on project notes" with a general-purpose embed mechanism that benefits any typed collection, not just projects.
