---
status: in-progress
phase: P5
parent_plan: ./001-task-in-tolaria.md
depends_on:
  - ./001-p3-view-engine.md
  - ./001-p4-board-view.md
---

# Plan 001 — P5 — Table view

## Goal

When a saved view declares `display: table`, render a dense, sortable table whose columns are driven by `view.definition.columns: string[]`. Reuses the same wide canvas slot the board uses, sharing the layout swap so a single resize handle controls the editor detail column. Per the [master plan](./001-task-in-tolaria.md#p5--table-view-12-days) this is a ~1–2 day phase.

## Decisions (locked)

- **UI slot:** same flex-1 canvas as the board. Generalize the `.app__board` class introduced in P4 to `.app__view-canvas` (the editor-shrink CSS keys off this class). Backward-compat: P4 board already shipped with `.app__board`; rename in the same commit since no public stylesheet depends on it.
- **Column source:** `view.definition.columns: string[]`. Each entry is a field name resolved through the same namespace rules as filters: `title`, `status`, `priority`, `due`, `start`, `completed`, `assignees`, `labels`, `project`, `estimate`, plus `file.X` derivations (`file.name`, `file.mtime`, `file.ctime`, `file.path`, `file.folder`, `file.ext`, `file.size`, `file.tags`). Bare frontmatter keys also work.
- **Defaults:** if `columns` is empty or missing, default to `['title', 'status', 'priority', 'due']`.
- **Cell rendering:** purely declarative. A small `columnCell.tsx` resolver decides which renderer to use for each field name:
  - `title` → text, click opens the entry; bold and truncated to one line
  - `status` → reuse `StatusPillCell` (read-only display mode for non-edit cells; we leave inline edit to P-future once we add a write back hook for tables)
  - `priority` → priority chip from `TaskCard`
  - `due` / `start` / `completed` → date badge using the same formatter as `TaskCard`
  - `assignees` / `labels` / `belongs_to` → comma-joined chip list
  - `project` → wikilink display (first relationship value, no `[[...]]` chrome)
  - `estimate` → numeric badge
  - `file.X` → text (mtime/ctime as relative time, size as bytes, others as raw)
  - Unknown bare field → string-coerced frontmatter property value
- **Inline editing v1:** defer. Keep the table read-only in v1 — the existing task editor on the right is the edit surface and is one click away. Re-enabling inline edits for status/priority/date can come in P5.5 once we have an `onUpdateFrontmatter` API mirroring the board's drop handler.
- **Sort:** click a column header → if same column is currently sorted, flip direction; otherwise apply that column's natural default direction (modified/created descending, everything else ascending). Persist the new sort via the existing `onUpdateViewDefinition(filename, { sort }, rootPath?)` callback (ADR 0040). Reuse `serializeSortConfig` from `noteListHelpers.ts`. Map the field name to a `SortOption` (e.g. `priority` → `property:priority`, `title` → `title`, `file.mtime` → `modified`).
- **Sticky header:** `position: sticky; top: 0; z-index: 1; background: ...` on the `<thead>` cells.
- **Empty state:** `tasks.table.emptyView` copy (mirrors the board).
- **Row click:** opens the entry in the editor via the same `onSelectNote` path as the board. No bulk-select or multi-row selection in v1.
- **PostHog event:** none for v1. Adding `task_view_switched` and `task_table_sorted` is queued for P17 (the analytics phase).

## Steps

### Step 1 — CSS rename: `.app__board` → `.app__view-canvas`

Touched: [src/App.css](../../src/App.css), [src/App.tsx](../../src/App.tsx). The selector `.app:has(.app__board) .app__editor` becomes `.app:has(.app__view-canvas) .app__editor`. The JSX wrapper around `TaskBoard` becomes `<div className="app__view-canvas">` and the new `<div>` wrapping `TaskTable` shares that class.

### Step 2 — Build `src/components/tasks/TaskTable.tsx`

```tsx
<TaskTable
  view={view}
  filteredEntries={filteredEntries}
  allEntries={allEntries}                  // not strictly needed v1, kept for parity
  selectedEntryPath={selectedEntryPath}
  onSelectNote={onSelectNote}
  onUpdateViewDefinition={onUpdateViewDefinition}
  locale={locale}
/>
```

- Inner helpers:
  - `resolveColumnLabel(name, locale)` — l10n-aware label for the column header.
  - `renderCell(entry, name, locale)` — the per-field cell renderer.
  - `mapColumnToSortOption(name)` — column → SortOption mapping for click-to-sort.
- Header `<th>` is clickable, shows direction indicator (▲ / ▼ / none).
- Sticky header.
- Tailwind: `min-w-full text-sm`, alternating row hover, truncated cells.

### Step 3 — Extract per-cell renderers to `columnCell.tsx`

`src/components/tasks/columnCell.tsx` — exposes:
- `getColumnLabel(name, locale)`
- `renderColumnCell(entry, name, locale)`
- `getColumnSortOption(name)`

Reuses date formatting from `TaskCard`'s due-badge helper (extract `dueLabel` to a shared module).

### Step 4 — Wire `display: table` into App.tsx

Extend the same selector that picks `selectedBoardView` to also handle `'table'`. Replace the board-only check with a `selectedViewCanvas` memo that returns `{ kind: 'board', view } | { kind: 'table', view } | null`. JSX branches on `.kind`.

### Step 5 — Localization

New keys in `src/lib/locales/en.json`:
- `tasks.table.emptyView` — "No items in this view."
- `tasks.table.column.title` / `status` / `priority` / `due` / `start` / `completed` / `assignees` / `labels` / `project` / `estimate` / `file.name` / `file.mtime` / `file.ctime` / `file.path` / `file.folder` / `file.ext` / `file.size` / `file.tags`
- `tasks.table.unknownColumn` — for bare custom property names; the column header falls back to the raw name when no key matches.

Seed English placeholders into all 14 non-en locale catalogs (lara-cli still pending on a machine with creds).

### Step 6 — Unit tests

- `columnCell.test.tsx`:
  - Renders title, priority chip, due badge, status pill, chip list for relationships
  - Falls back to string-coerced property values for unknown bare names
  - Returns correct sort option for each known name
- `TaskTable.test.tsx`:
  - Renders default columns when `columns` is empty
  - Renders one row per filtered entry
  - Sticky header is in the DOM
  - Empty-state copy when filtered list is empty
  - Clicking a row calls `onSelectNote`
  - Clicking a column header calls `onUpdateViewDefinition` with the right serialized sort
  - Clicking the same header twice flips the direction

### Step 7 — Demo content

Add `demo-vault-v2/views/q2-launch-table.yml` — a table over the same Q2 Launch task set with `columns: [title, status, priority, due, assignees, file.mtime]`. Sample yaml lives alongside the existing board view.

### Step 8 — Docs

Update [docs/ARCHITECTURE.md](../ARCHITECTURE.md) and [docs/ABSTRACTIONS.md](../ABSTRACTIONS.md):
- ARCHITECTURE.md → after the Board view section, add a Table view paragraph describing the column-driven renderer, sort write-back path, and shared canvas.
- ABSTRACTIONS.md → add entries for `TaskTable`, `columnCell.tsx`, and any extracted shared modules.

### Step 9 — QA + commit + push

- `npx tsc --noEmit` clean
- `pnpm lint` clean
- `pnpm test --silent` — full FE suite green
- `pnpm test:coverage --silent` — frontend ≥70%
- `pnpm tauri dev` → open the demo table view → click headers, verify sort writes to YAML, verify row-click opens in the editor.
- Push (pre-push runs Playwright smoke + Rust + CodeScene gate). **Never `--no-verify`.**

## Out of scope

- Inline-editable cells (deferred to P5.5).
- Row drag-reorder, multi-select, bulk actions.
- Column resize, column hide/show UI (the YAML is the source of truth).
- Pinned / frozen first column.
- Pagination — entries are filtered already; v1 renders all rows in the canvas.
- Virtualization. If a board view balloons past ~500 visible rows we should revisit, but v1 keeps it simple.
