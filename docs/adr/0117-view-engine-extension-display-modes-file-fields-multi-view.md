---
type: ADR
id: "0117"
title: "View engine extension: display modes, file metadata fields, multi-view files"
status: active
date: 2026-05-12
supersedes: "0040"
---

## Context

[ADR 0040](0040-custom-views-yml-filter-engine.md) defines saved views as `.yml` files in `.laputa/views/`, with a filter tree (`all` / `any` groups of conditions), sort string, optional icon and color, and a field resolver that falls back from a fixed built-in set (`type`, `status`, `title`, `archived`, `trashed`, `favorite`) to frontmatter and relationships. Views render in the sidebar as a flat note list.

The Tasks feature ([ADR 0115](0115-tasks-and-projects-as-typed-notes.md)) requires three things the current engine does not support:

1. **Multiple display modes.** A board, a table, and a timeline are all valid renderings of the same task collection. A view should declare which one it wants.
2. **File metadata as a queryable field.** "Tasks modified this week" needs `file.mtime`; "tasks tagged `urgent`" needs `file.tags`. The current resolver knows only a handful of fields plus a frontmatter fallback.
3. **Multiple views per file.** A project note's companion view file naturally wants a board, a table, and an "Overdue" filter together — not three separate `.yml` files.

The closest precedent is Obsidian Bases, which uses a three-namespace property model (`note.X`, `file.X`, `formula.X`) and supports multiple views per `.base` file. Adopting the namespace convention keeps the mental model accessible to users coming from Bases and leaves the door open for `.base` file format compatibility in a future ADR — without committing to the full Bases formula DSL, which is a much larger surface than v1 needs.

The v1 cut deliberately stays narrow: the structured filter tree (`field` / `op` / `value` conditions inside `all` / `any` groups) stays exactly as today because the UI generates filters from it; only the field resolver, the display configuration, and the file format gain new capabilities.

## Decision

**The `ViewDefinition` schema gains optional `display`, `group_by`, and `columns` fields; the field resolver accepts the `note.X` / `file.X` / `formula.X` namespace prefixes with a locked v1 capability set; view files may optionally contain multiple views via a top-level `views:` array. ADR 0040's storage location, filter tree shape, and operator set remain unchanged.**

Specifically:

1. `ViewDefinition` gains three optional fields:
   - `display: list | table | board | timeline | cards` (default `list`)
   - `group_by: { property: string, direction?: ASC | DESC }` (required when `display: board`; optional elsewhere; when `display: timeline`, controls swimlane grouping)
   - `columns: [string]` (used by table view to declare visible columns and order; absent means "all known properties")
2. The field resolver in `src-tauri/src/vault/views.rs` routes by namespace prefix:
   - `note.<name>` → frontmatter property (the existing fallback behavior, now explicit)
   - `file.<name>` → built-in file metadata (locked list in decision 3)
   - `formula.<name>` → reserved; parsing succeeds but evaluation always errors with `"v2 feature: formula properties not implemented"` so users see the boundary at view-open time, not at save time
   - Bare names (no prefix) keep resolving to `note.<name>` for back-compat with existing user views in `.laputa/views/`
3. `file.X` fields available in v1 (locked):
   - `file.name` (string, filename with extension)
   - `file.basename` (string, filename without extension)
   - `file.path` (string, vault-relative)
   - `file.folder` (string, parent folder path)
   - `file.ext` (string)
   - `file.size` (number, bytes)
   - `file.ctime` (date, start-of-day-UTC normalized per existing convention)
   - `file.mtime` (date, start-of-day-UTC normalized)
   - `file.tags` (list of strings, from frontmatter `tags:` only — NOT inline `#tag` content, which is a v2 scan-cost question)
4. Filter expressions may use a locked set of v1 helper functions:
   - `today()` → current date, start-of-day UTC
   - `now()` → current datetime UTC (used with full-datetime fields like `due` with time, or with `github_last_synced`)
   - Duration arithmetic: `+` and `-` between a date and a duration string. Duration strings: `"1d"`, `"1 day"`, `"2 weeks"`, `"3 months"`, `"1 year"`. Reuses the existing relative-date parser from [src-tauri/src/vault/view_date_filters.rs](../../src-tauri/src/vault/view_date_filters.rs).
   - `file.hasTag(tag)` and `file.hasTag(a, b, ...)` (variadic, any-match against frontmatter `tags:`)
   - `file.inFolder(path)` (true for direct match and any subfolder match)
   - `contains(haystack, needle)` (case-insensitive substring match)
   - Any other function name produces a parse-time error: `"v2 feature: function '<name>' not implemented"`. The error message is part of the schema contract — users see it before they hit save on a malformed view, and the boundary between v1 and v2 capabilities is discoverable without reading docs.
5. View files may use either the single-view format (current behavior) or a new multi-view format. Detection: if the loaded YAML's top-level keys include `views`, the parser treats it as multi-view; otherwise single-view. Each entry in the `views:` array is a `ViewDefinition`-shaped body (per-view `name`, `icon`, `color`, `display`, `group_by`, `columns`, `sort`, `filters`). A `default: true` flag on one entry marks the default view used when the file is embedded without a `#viewname` anchor ([ADR 0118](0118-embeddable-view-files-in-notes.md)).
6. Round-trip preserves the loaded shape. Re-serializing a single-view file produces a single-view file; re-serializing a multi-view file produces a multi-view file. The parser never silently upgrades or downgrades the format.
7. `note.X` and `file.X` namespace prefixes are accepted in the existing `field:` slot of structured filter conditions (`{ field: "file.mtime", op: "after", value: "today() - 1 week" }`). The parser splits on the first `.` to determine namespace.

## Alternatives considered

- **Extend `ViewDefinition` and adopt namespace prefixes** (chosen): a backwards-compatible superset of ADR 0040 that gets tasks the display modes and metadata access they need without forking the engine. Lowest mental-model cost for users.
- **Build a parallel tasks-only view system**: faster initial implementation (no back-compat concerns), but fragments the codebase and prevents reuse — a "Notes I touched this week" view would need a totally different code path from "Tasks I touched this week". Rejected.
- **Adopt Obsidian Bases YAML wholesale**: would give us the full formula DSL, summary aggregations, and `.base` file compatibility in one step, but the surface is large (multi-month implementation) and the structured filter tree we already use is better-suited to UI-generated filters. Rejected for v1; the namespace adoption keeps the door open for a future compatibility ADR.
- **Skip `file.X` fields and require everything via frontmatter**: users would have to manually maintain `modified:` / `tags:` frontmatter properties — a permanent papercut. Rejected.
- **Allow `formula.X` in v1**: requires a full expression parser, evaluator, and type system. That is its own multi-week feature; v1 keeps the function set minimal so the surface stays small. Rejected.
- **Open-ended function set**: would let users write `file.backlinks.length` or `sum(estimate)` style queries today. Rejected because each function is a performance and correctness contract that deserves an ADR; opening the gate produces a long tail of partial implementations. The "v2 feature: <name>" error path is intentionally the discoverability mechanism.

## Consequences

- The field resolver gains namespace-prefix routing. Bare names keep resolving to `note.X` so every existing user view in `.laputa/views/` continues to render exactly as before.
- A new deserialization branch in `vault/views.rs` distinguishes single-view from multi-view files. Existing single-view files do not migrate; they stay in their current shape until a user opts in to multi-view by editing the file.
- View files that reference `formula.X` or call unknown functions produce a friendly error at view-open time rather than silently failing or rendering an empty result. This is the primary mechanism for users to discover the v1/v2 boundary.
- `file.X` fields are computed on every filter evaluation. Caching is the resolver's responsibility; loading `file.tags` for a vault with 10k notes happens once per session, not once per filter row.
- Multi-view files become the natural home for project-companion views ([ADR 0118](0118-embeddable-view-files-in-notes.md) embeds them inline via `![[my-project-views.yml#board]]`).
- A future ADR may revisit: full `.base` file compatibility, formula expression language with dot-method chains (`[1,2,3].filter(value > 2)`), summary aggregations (`Sum`, `Average`, `Earliest`), and inline `#tag` scanning for `file.tags`.
- Re-evaluate if: users start hand-writing complex filter expressions that the structured `field`/`op`/`value` shape can't express cleanly, which would suggest the time has come for the full expression parser.
