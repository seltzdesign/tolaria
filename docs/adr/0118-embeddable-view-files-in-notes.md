---
type: ADR
id: "0118"
title: "Embeddable view files in notes"
status: active
date: 2026-05-12
---

## Context

Project notes need to display a board, table, or timeline of the project's tasks inline alongside the project README. The simplest implementation would be a hard-coded "if `type: project`, auto-render the board above the body" hook, but that pattern is fragile — it works only for projects, can't compose multiple views in one note, and ties view rendering to a specific type.

A general embedding mechanism is the right abstraction. A code search confirms Tolaria has no transclusion or `![[X]]` rendering today, so there is no existing syntax to conflict with. Obsidian users — a meaningful slice of Tolaria's prospective audience — already expect `![[file]]` to render an inline embed. Matching that syntax is low-cost (greenfield surface) and high-value (mental model preservation). The extended view engine ([ADR 0117](0117-view-engine-extension-display-modes-file-fields-multi-view.md)) already supports multi-view files, so the embed mechanism only needs to specify how a particular view is selected from a file and how the embedded view sees its context.

The "scoped to this project" use case is the entire reason embeds exist for Tasks: a project note's body contains `![[project-views.yml#board]]`, and the embedded board filters to tasks whose `project` field equals the embedding note. That requires the embedded view to know which note it's embedded in — a context object available to filter expressions at render time.

## Decision

**Tolaria introduces transclusion syntax `![[target]]` and `![[target#section]]` in the editor's markdown renderer. v1 supports exactly one target type — `.yml` view files — and exposes a `this` context object to the embedded view's filters that resolves to the embedding note's properties.**

Specifically:

1. The markdown renderer parses `![[X]]` and `![[X#Y]]` as transclusion. When `X` resolves to a `.yml` file in `.laputa/views/` (or anywhere via the wikilink resolver — see decision 3), it renders an inline embedded view. When `X` resolves to a `.md` file, it renders a placeholder `"v2 feature: note transclusion"` so the syntax is reserved and consistent but the implementation stays scoped to v1.
2. View selection:
   - `![[view.yml]]` renders the default view from the file. For a single-view file ([ADR 0117 §5](0117-view-engine-extension-display-modes-file-fields-multi-view.md)) this is the only view; for a multi-view file it is the entry marked `default: true`, falling back to the first entry in the `views:` array if no entry is marked.
   - `![[view.yml#viewname]]` renders the named view from a multi-view file. If the name does not match any entry, the embed renders a `"view '<name>' not found"` placeholder with a link to the view file.
3. File resolution uses the existing path-suffix wikilink resolver from [ADR 0035](0035-path-suffix-wikilink-resolution.md). View files in `.laputa/views/` are discoverable by basename alone (`![[my-project-views.yml]]`) or by partial path (`![[Projects/my-project-views.yml]]`) for disambiguation.
4. The embedded view's filter expressions can reference a `this` context object whose properties resolve to the **embedding note**, not the view file:
   - `this.file.name`, `this.file.basename`, `this.file.path`, `this.file.folder`, `this.file.ext`, `this.file.ctime`, `this.file.mtime`, `this.file.tags` — file metadata of the embedding note ([ADR 0117 §3](0117-view-engine-extension-display-modes-file-fields-multi-view.md))
   - `this.note.<X>` — frontmatter property `<X>` of the embedding note
   - When the view file is opened in a tab directly (not embedded), `this` falls back to the view file's own properties so filters using `this` still resolve to something coherent at edit time.
5. `this` references are usable in any filter condition's `value` slot: `{ field: "project", op: "equals", value: "this.note.title" }` resolves at render time against the embedding note. Equivalent for namespaced fields (`this.file.folder` etc.).
6. Embed rendering is read-only with respect to the view definition itself — users can't edit the view's filters or sort order from inside the embed; that happens by opening the view file in a tab. But edits to the data items shown inside the embed (changing a task's status from an embedded board, renaming a task inline) DO work and propagate directly to the underlying task `.md` files. The view file is the lens; the items are the data.
7. Nested embeds are not supported in v1. A view embedded in a note must not itself render an embedded view. Detection uses a render-depth counter passed through the embed component tree; depth > 1 renders a `"v2 feature: nested embeds"` placeholder.
8. Embed size caps (v1 constants, not user-configurable):
   - Table / list: 200 rows
   - Board: 5 columns × 50 cards
   - Timeline: 100 items
   - Above the cap, the embed renders the first N items and appends a `"Showing first N — open view to see all"` footer linking to the view file.

## Alternatives considered

- **`![[view.yml]]` syntax with a `this` context object** (chosen): leverages an established convention (Obsidian transclusion) without forking it for a Tolaria-specific feature, and makes "scoped to this project" composable in any note, not just project notes.
- **Auto-render hook keyed off `type: project`**: simpler to implement (no parser change) but project-specific. Can't show two different views in one project note (e.g., a board plus an "Overdue" table). Can't show a view in a non-project note (e.g., a weekly review note that aggregates "tasks due this week" across projects). Rejected.
- **Dedicated fence syntax (```view ... ```)**: avoids any chance of confusion with future note transclusion and could carry inline configuration. Rejected because we'll need `![[X]]` for note transclusion eventually and two parallel inline-rendering systems are worse than one. The cost of reserving `![[X.md]]` as a placeholder until v2 is small.
- **Build full note transclusion in v1**: gives `![[note.md]]` first-class meaning immediately, but note transclusion has its own design space — partial sections (`![[note#heading]]`), block references, recursive embeds, edit propagation — that v1 doesn't need to solve. Rejected as scope creep; the `.md` placeholder is the explicit deferral.
- **No `this` context object**: embedded views always use their own filters. Rejected because it kills the "scoped to this project" use case — every project would need its own dedicated view file with hardcoded filter values, which fights the "one project README references one shared multi-view file" pattern.

## Consequences

- The markdown renderer gains a parsing pass for `![[X]]` and `![[X#Y]]` tokens. v1 implementation routes `.yml` to the embedded view component and renders a placeholder for everything else. The token never falls through to the default markdown text path.
- A new component `<EmbeddedView />` accepts a view file path, an optional view name, the embedding note's path, and a render-depth counter. It loads the view via the standard view loader, constructs the `this` context, and renders the appropriate display component (list / table / board / timeline / cards).
- `this` references in filter expressions add a small per-row evaluation cost — the context object is constructed once per embed render, not per row, so the cost is bounded.
- The wikilink resolver from [ADR 0035](0035-path-suffix-wikilink-resolution.md) gains a "what file extension is this" branch in callers so they route to the right embed handler (view file vs. note placeholder).
- Editing data items inside an embed exercises the same save path as editing them in their own tab — the embed is a lens, not a separate edit surface — so there is no new persistence story to design.
- v1 explicitly does not support: nested embeds (placeholder), `.md` transclusion (placeholder), partial-section embeds like `![[note#heading]]` (placeholder), live size cap configuration. Each is a separate v2 ADR.
- Re-evaluate if: users start asking for partial-section embeds for notes (which would imply note transclusion has become the more common embed shape than view transclusion), or if `this`-context filters become a substantial fraction of view CPU time.
