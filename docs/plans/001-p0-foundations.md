# Plan 001 / P0 — ADRs and Schema Lock-in

> Parent plan: [001-task-in-tolaria.md](./001-task-in-tolaria.md)
> This file expands P0 only.

## Context

P0 is the foundational phase of the Tasks-in-Tolaria feature. No production code changes in P0 — its outputs are five ADRs and three locked schemas (task frontmatter, project frontmatter, extended `ViewDefinition`). Everything in Tracks B–E builds on these, and ADRs are deliberately a one-way door: once a schema is shipped, changing it means a migration. The whole point of P0 is to make the irreversible choices on purpose, not by accident in Phase 1's first hour.

Two findings from recent exploration that change earlier assumptions:

1. **Tolaria already uses `type:` not `is_a:` for the type field** ([ADR 0025](../adr/0025-type-field-canonical.md)). The parent plan's frontmatter examples use `is_a:` throughout — wrong. Every reference becomes `type:`. This is a strict find-and-replace in the parent plan (Phase 1 cleanup item; out of scope for P0 itself but called out so we don't re-introduce the mistake in the ADRs).
2. **Status is already free-form** with `SUGGESTED_STATUSES` in [src/lib/statusStyles.ts](../../src/lib/statusStyles.ts). No enum constraint exists in the backend. We do NOT introduce one — task status remains a free string, and each bound project's allowed values come from the GitHub Project's Status field options. This simplifies P0 vs. what the parent plan implied.

## Decisions locked in P0

**User-chosen** (from earlier AskUserQuestion):

1. **PAT type:** support BOTH fine-grained PAT (`github_pat_*`) and classic PAT (`ghp_*`) — detect from prefix.
2. **Filename convention for synced tasks:** title-based with rename tracking — matches [ADR 0007](../adr/0007-title-filename-sync.md) and uses [ADR 0075](../adr/0075-crash-safe-note-rename-transactions.md) for crash safety. Collisions resolve via `(2)`, `(3)` suffix.
3. **Embed syntax:** `![[view.yml]]` and `![[view.yml#viewname]]` — Obsidian-compatible. Tolaria has no transclusion today; this is greenfield.

**Author-chosen** (committed in P0 without further user questions, but called out explicitly so the user can override at review):

4. **Type field naming:** `type: task` and `type: project`. Lowercase slug. Matches [ADR 0025](../adr/0025-type-field-canonical.md). NOT `is_a:`.
5. **Dates:** `due`, `start`, `completed` accept ISO 8601 date (`YYYY-MM-DD`) or full RFC 3339 datetime (`YYYY-MM-DDTHH:MM:SS±HH:MM` / `...Z`). Time is optional. Offset is mandatory on datetime values (so vaults stay portable across timezones via git); UI-written datetimes always include the user's current offset. Day-granularity filters from [ADR 0048](../adr/0048-relative-date-expressions-in-view-filters.md) ignore the time component, preserving back-compat with existing date filter machinery in [view_date_filters.rs](../../src-tauri/src/vault/view_date_filters.rs). Sync to GitHub Projects v2 drops time (GH date fields are date-only) — sync log warns once per task. Datetime support unlocks future iCal/CalDAV/Google Calendar sync with no schema migration.
6. **Status vocabulary:** free-form per project. Defaults match `SUGGESTED_STATUSES` (`Not started`, `In progress`, `Done`, `Blocked`, etc.). When bound to a GitHub Project, the project note's `statuses:` array mirrors the GH Status field options exactly.
7. **Tasks without projects are valid.** Standalone tasks just have no `project:` field (and no GH binding). The Tasks home view shows all tasks regardless. Avoids the "create a project first to add one todo" friction.
8. **Per-project task folder.** Each project note declares `task_folder:` in its frontmatter (default: same folder as the project note). Synced tasks pulled from GitHub write into this folder. Existing tasks moved into this folder don't get reassigned — they keep their `project:` wikilink as the source of project membership.
9. **Conflict tiebreaker:** when local and remote timestamps are equal during LWW reconciliation, **remote wins**. Rationale: remote ties are rare (would need same-second edits in disconnected state); favoring remote keeps the github.com view authoritative when in doubt and is the less-surprising default for users who think of GitHub as the "real" project tracker.
10. **v1 filter function list (locked):** `today()`, `now()`, duration arithmetic on dates (`+`, `-` with `"1 day"`, `"2 weeks"` etc.), `file.hasTag(...)`, `file.inFolder(...)`, `contains(haystack, needle)`. Anything else (e.g., `formula.X`, `file.backlinks`, `sum()`, `if()`, dot-method chains) errors at parse time with `"v2 feature: <name>"`. The error message is part of the schema contract.
11. **Multi-view file back-compat:** existing single-view `.yml` files keep parsing exactly as today. New `display`, `group_by`, `columns` fields are optional, default `display: list`. Multi-view files use a top-level `views:` array; presence of that array switches the parser into multi-view mode. Round-trip preserves whichever shape was loaded.
12. **`note.X` / `file.X` / `formula.X` namespace adoption:** bare names continue to resolve to `note.X` (frontmatter) for back-compat; the prefixed forms are new and explicit. `formula.X` parses but always errors with the v2 message above.

## The five ADRs

ADR numbering: next available number is 0115 (per the most recent ADR found, 0114 "Mounted workspaces unified graph"). So ADRs 0115–0119.

| File | Purpose | Supersedes |
|---|---|---|
| `docs/adr/0115-tasks-and-projects-as-typed-notes.md` | Frontmatter schema lock | — |
| `docs/adr/0116-github-projects-bridge-narrow-exception-to-0056.md` | PAT auth + scope | 0056 (partial) |
| `docs/adr/0117-view-engine-extension-display-modes-file-fields-multi-view.md` | Extended `ViewDefinition` | 0040 (partial) |
| `docs/adr/0118-embeddable-view-files-in-notes.md` | `![[view.yml]]` syntax | — |
| `docs/adr/0119-three-way-diff-lww-for-projects-sync.md` | Conflict policy | — |

ADR house style (from recent ADRs): frontmatter `type: ADR / id / title / status: active / date / supersedes (optional)`, body sections `Context / Decision / Alternatives considered / Consequences`, numbered decision points, citation chain to prior ADRs.

### ADR 0115 — Tasks and projects as typed notes

**Frontmatter date:** today.

**Context outline:**
- Tolaria is adding a task/project management feature inspired by Notion and GitHub Projects.
- Existing options: parallel `Task` struct alongside `VaultEntry`, or treat tasks as a flavor of note.
- [ADR 0002](../adr/0002-filesystem-source-of-truth.md) (filesystem-as-truth) and [ADR 0025](../adr/0025-type-field-canonical.md) (`type:` canonical) constrain the answer.

**Decision (numbered):**
1. A task is a `VaultEntry` with `type: task` in its frontmatter — no new struct, no separate index.
2. A project is a `VaultEntry` with `type: project`. Project notes additionally store binding metadata for the GitHub Projects bridge.
3. Both types ship as **starter type documents** in the vault root (`task.md`, `project.md`), per [ADR 0096](../adr/0096-root-created-type-documents.md). Users can customize these per-vault (rename, add custom fields).
4. The task frontmatter schema is locked as below. Adding fields later is allowed (forwards-compat); renaming or removing requires a new ADR and migration.

```yaml
# Task frontmatter schema (v1)
---
type: task                        # required, literal "task"
title: "Implement task drag-drop" # optional; falls back to H1 / filename per ADR 0044
status: "In progress"             # free-form string; UI suggests from project's statuses or SUGGESTED_STATUSES
priority: P1                      # optional, free string; UI suggests P0|P1|P2|P3
due: 2026-05-20                   # date OR datetime — `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SS±HH:MM`
# due: 2026-05-20T14:00:00+02:00  # ...with time + timezone offset
start: 2026-05-15                 # date OR datetime
completed: 2026-05-18             # date OR datetime (set when status moves to "Done")
assignee:                         # optional, list of wikilinks or @gh-usernames
  - "[[Armin]]"
project: "[[My Cool Project]]"    # optional; wikilink to a project note. Absence == standalone task.
blocked_by:                       # optional; tasks that must complete before this one (local-only in v1)
  - "[[Set up CI]]"
labels: [bug, frontend]           # optional, list of strings
estimate: 3                       # optional, number (story points / hours)

# Bridge-managed fields (written by sync engine):
github_project_node_id: PVT_kwHO...
github_item_node_id: PVTI_lAHO...
github_issue_url: https://github.com/...   # only when project.link_to_issues == true
github_sync_status: synced                 # synced | local-only | conflicted | error
github_last_synced: 2026-05-12T14:30:00Z   # ISO 8601 datetime
github_remote_snapshot_hash: a1b2c3d4      # for 3-way conflict detection (see ADR 0119)
---

# H1 = title (per ADR 0044, ADR 0055, ADR 0068)

Free-form markdown body. Becomes the GH Project item's body / linked issue body when synced.
```

```yaml
# Project frontmatter schema (v1)
---
type: project
title: "My Cool Project"

# GitHub Projects binding (optional — set when bound)
github_project_url: https://github.com/users/seltzdesign/projects/7
github_project_node_id: PVT_kwHO...
sync_enabled: true
sync_interval_minutes: 5
link_to_issues: false                   # if true, syncing creates real Issues, not draft items
github_issue_repo: seltzdesign/some-repo # required iff link_to_issues == true

# Task scope (always set)
task_folder: "Projects/My Cool Project/tasks"  # default: same folder as this project note
statuses:                                       # default: SUGGESTED_STATUSES; mirrors GH Status field
  - "Not started"
  - "In progress"
  - Done
terminal_statuses: [Done]                       # optional; default: [Done] — statuses that count as "blocked_by satisfied"

# Field mapping (set when bound to a GH Project)
status_field: Status                            # which GH custom field maps to local `status`
field_mappings:
  priority: Priority                            # local frontmatter key → GH field name
  due: "End date"
  start: "Start date"
  estimate: Estimate

# View defaults
default_view: board
---

# H1 = project title

Project README content goes here. Will typically embed a view (P7):

![[my-cool-project-board.yml]]
```

**Alternatives considered:**
- New top-level `Task` struct alongside `VaultEntry` (rejected — fights the vault-is-source-of-truth model, duplicates persistence/cache machinery, makes tasks invisible to existing search/wikilink).
- Parallel `.tasks/` folder with custom format (rejected — overloads cache directory, breaks "your vault is just `.md` files").
- Use `is_a:` instead of `type:` (rejected — contradicts [ADR 0025](../adr/0025-type-field-canonical.md)).

**Consequences:**
- Tasks are wikilink-able, searchable, renderable in any markdown editor.
- Bridge fields pollute the frontmatter of synced tasks. Mitigation: `github_` prefix groups them visually; in v2, consider a nested `_github:` object.
- The `task` and `project` type docs are user-editable (per [ADR 0096](../adr/0096-root-created-type-documents.md)). Adding a custom property to the type doc propagates to all new tasks.
- Triggers re-evaluation if: frontmatter grows past ~30 fields and YAML readability degrades.

### ADR 0116 — GitHub Projects v2 bridge: narrow exception to ADR 0056

**Frontmatter:** `supersedes: "0056"` — partial supersession; ADR 0056's principle remains in force everywhere outside the Projects bridge.

**Context outline:**
- [ADR 0056](../adr/0056-system-git-cli-auth-no-provider-oauth.md) removed all GitHub-specific auth and API code from Tolaria in favor of system git credentials.
- The Tasks feature requires bidirectional sync with GitHub Projects v2, which is GraphQL-only and requires a token. The system-git path cannot reach the Projects API.
- The exception needs to be tightly scoped so the rest of the app stays on the "system git only" path.

**Decision (numbered):**
1. Tolaria re-introduces GitHub API auth, but exclusively for the GitHub Projects v2 bridge — never for git transport (commit/push/pull continue to use system git).
2. The PAT is stored in the OS keychain via the `keyring` crate (service name `com.tolaria.app.github_pat`). It does NOT live in `settings.json`, `~/.config/tolaria/`, any vault file, or any log.
3. Tolaria accepts both fine-grained PATs (`github_pat_*` prefix, recommended) and classic PATs (`ghp_*` prefix). Settings UI surfaces required scopes for each: fine-grained needs `Projects: read/write` + chosen-repo `Issues: read/write`; classic needs `repo` + `project` scopes.
4. The bridge is opt-in per project (project note's `sync_enabled: true`). A user with no projects bound has no functional difference from a no-PAT install — the bridge module loads but never makes a request.
5. PAT presence is a precondition for binding; "Test connection" must return a valid `viewer { login }` before the Bind Project flow opens.
6. PAT rotation is the user's responsibility. The app warns (non-blocking) when a `viewer { login }` request returns 401 and prompts the user to re-enter.

**Alternatives considered:**
- Keep ADR 0056 in force; tell users to manage Projects via the CLI (`gh project ...`) (rejected — defeats the purpose of in-app task UI).
- Store PAT in `settings.json` (rejected — secrets in plain text on disk are unacceptable).
- Use GitHub Device Flow OAuth like the original [ADR 0019](../adr/0019-github-device-flow-oauth.md) (rejected — large auth stack for a single integration; PAT entry is simpler).
- Fine-grained only (rejected — adds friction for users with existing classic PATs).

**Consequences:**
- New crate dep: `keyring = "3"`.
- New module `src-tauri/src/github/projects/` with its own auth, client, sync, scheduler.
- The bridge is the ONLY code path that calls `api.github.com`. All git operations continue via system `git`.
- Telemetry: PAT contents never logged. Token type (fine-grained vs classic) IS sent as an anonymous event property.
- Loss-of-PAT recovery: user re-enters in Settings; existing project bindings stay valid since they store project node IDs, not credentials.

### ADR 0117 — View engine extension: display modes, group-by, `file.X` fields, multi-view files

**Frontmatter:** `supersedes: "0040"` — partial; ADR 0040's `.yml`-in-`.laputa/views/` storage and filter operators stay; only the schema is extended.

**Context outline:**
- [ADR 0040](../adr/0040-custom-views-yml-filter-engine.md) defines saved views as `.yml` files in `.laputa/views/`, with `name`, `icon`, `color`, `sort`, `filters` (all/any tree).
- The Tasks feature requires: (a) different display modes (board, table, timeline) per view, (b) `file.X` metadata in filters, (c) multiple views per file.
- Obsidian Bases uses a similar three-namespace property model (`note.X` / `file.X` / `formula.X`); adopting it keeps the mental model accessible to Bases users and leaves the door open for `.base` file compatibility in v2.

**Decision (numbered):**
1. `ViewDefinition` is extended with three optional fields:
   - `display: list | table | board | timeline | cards` (default `list`)
   - `group_by: { property: string, direction?: ASC | DESC }` (required when `display: board`; optional elsewhere)
   - `columns: [string]` (used by table view to declare visible columns and order)
2. The field resolver in `vault/views.rs` accepts three namespaces:
   - `note.<name>` → frontmatter property (existing behavior, now explicit)
   - `file.<name>` → built-in file metadata (locked list below)
   - `formula.<name>` → reserved; parses but always errors with `"v2 feature: formula properties not implemented"`
   - Bare names (`status`, `due`, etc.) keep resolving to `note.X` for back-compat
3. `file.X` fields supported in v1 (locked):
   - `file.name`, `file.basename`, `file.path`, `file.folder`, `file.ext`
   - `file.size` (number, bytes)
   - `file.ctime`, `file.mtime` (date — start of day, in line with existing date filter convention)
   - `file.tags` (list of strings, from frontmatter `tags:` only — NOT inline `#tag` content; v2 stretch)
4. v1 filter helper functions (locked):
   - `today()` → current date, start-of-day UTC
   - `now()` → current datetime UTC
   - Duration arithmetic: `+` and `-` between a date and a string duration. Duration strings: `"1d"`, `"1 day"`, `"2 weeks"`, `"3 months"`, `"1 year"`. Reuses existing relative-date parsing from `view_date_filters.rs`.
   - `file.hasTag("tag")` and `file.hasTag("a", "b")` (variadic, any-match)
   - `file.inFolder("Projects/Active")` (true for direct + subfolder match)
   - `contains(haystack, needle)` (string contains; case-insensitive)
   - Any other function name → parse-time error `"v2 feature: function '<name>' not implemented"`
5. Multi-view file format: a view file MAY use a top-level `views:` array; each element is a `ViewDefinition`-shaped body. Detection: if top-level keys include `views`, treat as multi-view; otherwise single-view. Round-trip preserves the loaded shape.
6. `note.X` / `file.X` namespace prefixes are accepted in the existing `field:` slot of structured filter conditions. The parser splits on `.` to find the namespace.

**Alternatives considered:**
- Build a parallel "tasks views" system (rejected — fragments the codebase; tasks should be queryable through the same views as notes).
- Adopt Obsidian Bases YAML wholesale (rejected for v1 — large surface area; left as v2 door for `.base` parity).
- Skip `file.X` fields entirely (rejected — "tasks I modified this week" needs `file.mtime`).
- Allow `formula.X` in v1 (rejected — formulas need a full expression parser + evaluator + type system).

**Consequences:**
- The field resolver gains namespace-prefix routing. Bare names keep resolving to frontmatter for back-compat.
- New deserialization branch for multi-view files; existing single-view files unchanged.
- View files with `formula.X` produce a friendly error at view-open time.
- Future ADR can revisit: `.base` file compatibility, formula expression language, dot-method chains, summary aggregations.

### ADR 0118 — Embeddable view files in notes

**Context outline:**
- The Tasks feature needs project notes that show a board inline. Options: hard-code "if `type: project`, auto-render the board" or build a general embed mechanism.
- Tolaria has no transclusion / embed support today (confirmed by code search).
- Obsidian users expect `![[file]]` to render an embed inline. Matching this syntax is low-cost (greenfield) and high-value (mental model preservation).

**Decision (numbered):**
1. Introduce transclusion syntax `![[target]]` and `![[target#section]]` in the editor's markdown renderer. v1 supports ONE target type: `.yml` view files. `![[notename.md]]` parses but renders a "v2 feature: note transclusion" placeholder.
2. View embeds resolve as follows:
   - `![[view.yml]]` → renders the default view from the file (single-view → the only view; multi-view → first in array OR the one marked `default: true`)
   - `![[view.yml#viewname]]` → renders the named view from a multi-view file (404 placeholder if not found)
   - File-resolution path: same as wikilink resolution per [ADR 0035](../adr/0035-path-suffix-wikilink-resolution.md)
3. The embedded view receives a context object `this`:
   - `this.file.name`, `this.file.basename`, `this.file.path`, `this.file.folder`, `this.file.ext`, `this.file.ctime`, `this.file.mtime`, `this.file.tags` — properties of the embedding note
   - `this.note.<X>` — frontmatter properties of the embedding note
   - When opened in a tab directly (not embedded), `this` refers to the view file itself
4. `this` values are usable in filter conditions: `field: project, op: equals, value: "this.note.title"` resolves at render time.
5. Embed rendering is **read-only** for the view definition itself. Editing data items shown in the embed (e.g., changing a task status from an embedded board) DOES work — edits go directly to the task `.md` files.
6. Nested embeds are NOT supported in v1. Detected via render-depth counter; >1 → "v2 feature: nested embeds" placeholder.
7. Embed size caps (v1 constants, not user-configurable): 200 rows for table/list, 5 columns × 50 cards for board, 100 items for timeline. Above the cap, a "Showing first N — open view to see all" footer.

**Alternatives considered:**
- Auto-render hook keyed off `type: project` (rejected — fragile, project-specific, can't compose multiple views).
- Dedicated fence syntax (```view ...```) (rejected — diverges from Obsidian; we'd need `![[X]]` later anyway).
- Build full note transclusion in v1 (rejected — scope creep).
- No `this` context object (rejected — kills the "embed a board scoped to this project" use case).

**Consequences:**
- New parser pass in the editor for `![[X]]` syntax. v1 implementation handles `.yml`; the `.md` branch returns a placeholder.
- New component `<EmbeddedView />` that takes a view file path + optional view name + `this` context.
- `this` resolution adds a small per-row evaluation cost. Acceptable at v1 caps.
- Wikilink resolver gains a "what file extension is this" branch; uses [ADR 0035](../adr/0035-path-suffix-wikilink-resolution.md) machinery.

### ADR 0119 — Three-way diff with last-write-wins for GitHub Projects sync

**Context outline:**
- The bridge is bidirectional: local `.md` edits push to GitHub; GitHub edits pull back to local.
- Without a conflict policy, simultaneous edits can silently overwrite each other.

**Decision (numbered):**
1. Conflict detection uses a per-project snapshot stored at `<cache-dir>/github-sync/<project_node_id>.json` (cache, NOT vault — per [ADR 0024](../adr/0024-cache-outside-vault.md)). Snapshot is the last-successfully-synced state of each item: full field values + `github_remote_snapshot_hash`.
2. Per-item reconciliation:
   - `local_changed = (local mtime > snapshot.synced_at) || (frontmatter content hash differs from snapshot.hash)`
   - `remote_changed = (remote updatedAt > snapshot.synced_at)`
   - Both false: no-op. Local only: push. Remote only: pull. Both: conflict → LWW.
3. LWW comparison: file `mtime` (filesystem authoritative) vs. remote `updatedAt`. Newer wins; loser is preserved at `<cache-dir>/github-sync/conflicts/<task_id>.<timestamp>.md`.
4. **Tie-breaker:** remote wins. Rationale: ties are rare; favoring remote keeps github.com authoritative in genuine doubt.
5. Conflict audit: every conflict resolution writes a line to `.tolaria/sync-log.jsonl` (in vault, gitignored). Schema: `{timestamp, project_id, task_id, winner, local_mtime, remote_updated_at, local_hash, remote_hash, conflict_copy_path}`.
6. Conflict copy retention: indefinite. User can manually clear `<cache-dir>/github-sync/conflicts/`; the sync log preserves the metadata trail.
7. Body content (markdown body) IS included in conflict diff. Winner's body is kept; loser's body is in the conflict copy.
8. The local task's `github_sync_status` field is set to `conflicted` while a conflict copy exists unresolved; cleared on next successful reconcile cycle.

**Alternatives considered:**
- Manual resolution UI per conflict (deferred to v2 — UX work + state management; v1 LWW with recoverable copies is acceptable for personal use).
- CRDT-based merging (rejected — overkill; GitHub Project fields are not CRDT-friendly).
- "Local always wins" or "Remote always wins" (rejected — too easy to lose data on one side).
- Snapshot stored in vault (rejected — pollutes vault with sync state).

**Consequences:**
- Loss-of-cache is not catastrophic. On first sync after cache loss, EVERY task looks like "both changed". Mitigation: initial-sync-after-no-snapshot mode treats remote as the snapshot baseline. Document this.
- The sync log grows monotonically. v1 accepts this; v2 may rotate.
- v1 UX message when a conflict happens: "Conflict on '<task title>': remote version won — your local edit saved to <path>".

## Reusable existing utilities relevant to P0

Referenced in the ADRs themselves; no code changes in P0, but call them out so Phase 1 knows what to reuse:

- **Frontmatter parser** — [src-tauri/src/vault/frontmatter.rs](../../src-tauri/src/vault/frontmatter.rs) handles `status: StringOrList` etc.
- **Filename ↔ title sync** — [ADR 0007](../adr/0007-title-filename-sync.md), used by sync for the title-based filename convention.
- **Crash-safe rename** — [ADR 0075](../adr/0075-crash-safe-note-rename-transactions.md), used by sync pull on rename.
- **Cache outside vault** — [ADR 0024](../adr/0024-cache-outside-vault.md), used by sync for snapshots and conflicts.
- **Wikilink resolution** — [ADR 0035](../adr/0035-path-suffix-wikilink-resolution.md), reused by the embed resolver.
- **Date filter parsing** — [src-tauri/src/vault/view_date_filters.rs](../../src-tauri/src/vault/view_date_filters.rs) and [ADR 0048](../adr/0048-relative-date-expressions-in-view-filters.md). The v1 helper functions extend this.
- **Status suggestions** — `src/lib/statusStyles.ts` `SUGGESTED_STATUSES`. Reused as default `statuses:` array.
- **Type document loader** — per [ADR 0096](../adr/0096-root-created-type-documents.md). Starter type docs use the standard loader.

## Verification — how P0 is "done"

P0 has no code. Verification is doc review:

1. **ADR review pass:** all 5 ADRs merged to `main`. Each has standard frontmatter and the standard sections.
2. **Numbered decision points:** every ADR's Decision section uses numbered points so Phase 1+ code can reference by number (e.g., "per ADR 0117 §3, `file.tags` is frontmatter-only").
3. **Schema lock cross-check:** the task / project frontmatter and extended `ViewDefinition` YAML in ADRs 0115 + 0117 are the canonical source of truth.
4. **Filter function list pinned:** ADR 0117 §4 lists exactly the v1 functions. Any PR adding a new function must update or supersede this ADR.
5. **No production code:** `git diff main` shows only `docs/adr/0115-*` through `docs/adr/0119-*`. No `.rs`, `.tsx`, `.ts`, `.json` changes.
6. **CodeScene:** docs-only commit — no code health check needed.

## Out of scope for P0 (explicit)

- Any code changes outside `docs/adr/`. The frontmatter parser does not change in P0; the view resolver does not change in P0; no UI work in P0.
- Localization (`pnpm l10n:translate`) — no UI strings yet.
- PostHog events — no user-visible behavior to instrument.
- Playwright tests — no UI to test.
- `.codescene-thresholds` updates — docs-only commit.
- Updating the parent plan [001-task-in-tolaria.md](./001-task-in-tolaria.md) to replace `is_a:` with `type:` throughout — necessary but better done at Phase 1 start alongside the schema-driven code work, or as a tiny standalone commit immediately after the ADRs land.
- `.base` file compatibility — v2 stretch.
- Formula expression language, summary aggregations, dot-method chains — v2.
- Nested embeds, note transclusion, partial-section embeds (`![[note#heading]]`) — v2.
- Manual conflict resolution UI — v2.

## Estimated effort

**1.5–2 days** of focused work:

- ~3 hours: ADR 0115 (task/project schema)
- ~2 hours: ADR 0116 (PAT bridge)
- ~3 hours: ADR 0117 (view engine extension)
- ~2 hours: ADR 0118 (embeds)
- ~2 hours: ADR 0119 (sync conflict policy)
- ~1 hour: cross-review pass (correct citation chains)
- ~1 hour: commit, push, completion comment

Each ADR is a small commit on its own (`docs:` prefix per [AGENTS.md §1b](../../AGENTS.md)). Direct push to `main` per [ADR 0021](../adr/0021-push-to-main-workflow.md).

## After P0 — Phase 1 ready-state

When P0 is done, P1 (task & project type registration + frontmatter validation) starts with:

- Exact frontmatter schemas to validate against (ADR 0115)
- Exact list of date fields and their format
- Exact extended `ViewDefinition` shape (ADR 0117) — though no code in `views.rs` lands until P3
- Starter type document templates draftable directly from ADR 0115 YAML

No re-discussion of schema choices during Phase 1+. Changes require a new superseding ADR.
