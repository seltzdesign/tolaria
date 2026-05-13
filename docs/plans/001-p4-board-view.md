---
status: in-progress
phase: P4
parent_plan: ./001-task-in-tolaria.md
depends_on:
  - ./001-p3-view-engine.md
---

# Plan 001 — P4 — Board view + drag-drop

## Goal

When a saved view declares `display: board`, render a kanban-style board instead of the standard `NoteList`. Columns are the distinct values of `groupBy.property` (prefer the bound project's `statuses` list if present). Cards drag between columns; on drop, the dragged entry's frontmatter for the group-by field is rewritten via the existing save path.

Per [the master plan](./001-task-in-tolaria.md#p4--board-view--drag-drop-2-days), this is a ~2 day phase. Backend support already shipped in P3 — this is pure frontend wiring.

## Decisions (locked)

- **UI slot:** board renders in the existing **note-list column** (same slot as `PulseView`). User can drag the column wider with the existing resize handle. Full-bleed layouts are out of scope for v1.
- **Selecting a card:** clicking a card opens that entry in the editor via the existing `onSelectNote` path. No special card-detail overlay.
- **Column source:**
  1. If `groupBy.property` is `status` AND any visible entry has a `project` wikilink pointing to an entry with `isA: project` and a non-empty `statuses` array → columns = that project's statuses, in order.
  2. Otherwise → columns = the union of (a) distinct values of `groupBy.property` across the filtered entries, plus (b) an `"(unset)"` bucket for entries with no value.
- **Default `groupBy`:** if a view declares `display: board` but no `groupBy`, default to `{ property: 'status' }`.
- **Drag library:** `@dnd-kit/core` + `@dnd-kit/sortable` (already present in `package.json`). Use `DndContext` with `useDraggable` on cards and `useDroppable` on columns. No sortable inside columns for v1 — column order of cards is "as filtered" (respects `view.sort`).
- **On drop:** call `onUpdateFrontmatter(entry.path, groupByField, newValue)` (the same path used by `TaskHeader` cells in P2). For the `"(unset)"` column we write the property to `null` / removed entry.
- **Field naming:** `groupBy.property` can be `status`, `note.status`, `priority`, etc. When *reading*, use the existing resolver (just look up the property; strip a `note.` prefix). When *writing*, always strip a `note.` prefix (we never write to `file.X` — that would be a no-op and the UI should not present such a board view as valid; we'll defensively early-return for unsupported namespaces).
- **Non-task entries:** the card primitive degrades gracefully — title only, no priority/due/assignees if the entry is not a task. This lets the board view work for any typed collection.
- **PostHog event:** fire `task_status_changed` when a status drop changes the value (P17 will add `task_view_switched` etc., not yet).

## Steps

### Step 1 — Extend FE `ViewDefinition` TS type ([src/types.ts](../../src/types.ts))

Mirror the P3 BE additions:

```ts
export type ViewDisplay = 'list' | 'table' | 'board' | 'timeline' | 'cards'

export interface ViewGroupBy {
  property: string
  direction?: 'asc' | 'desc'
}

export interface ViewDefinition {
  // ...existing fields
  display?: ViewDisplay
  groupBy?: ViewGroupBy
  columns?: string[]
}
```

### Step 2 — Normalize new fields in [src/utils/vaultMetadataNormalization.ts](../../src/utils/vaultMetadataNormalization.ts)

Add defensive normalization in `normalizeViewDefinition`:

```ts
if ('display' in definition) normalized.display = normalizeViewDisplay(definition.display)
if ('groupBy' in definition) normalized.groupBy = normalizeViewGroupBy(definition.groupBy)
if ('columns' in definition) normalized.columns = stringArrayFrom(definition.columns)
```

Helpers reject unknown enum values (fall back to undefined, never throw).

### Step 3 — `useBoardColumns` derivation helper ([src/lib/tasks/boardColumns.ts](../../src/lib/tasks/boardColumns.ts))

Pure function: given the filtered entries, the `groupBy` property, and the full `entries` array (for looking up the bound project), return `BoardColumn[]`:

```ts
export interface BoardColumn {
  key: string                 // value used for `entry.properties[groupByField]`
  label: string               // display label (key, or `(unset)`)
  isUnset: boolean
  entries: VaultEntry[]
}

export function deriveBoardColumns(
  filtered: VaultEntry[],
  allEntries: VaultEntry[],
  groupBy: ViewGroupBy,
): BoardColumn[]
```

Algorithm:
1. Resolve the group-by field name (strip `note.` prefix, lowercase).
2. If field is `status`, walk `filtered` for the first entry with a project wikilink; resolve that wikilink against `allEntries`; if the target is a project with `statuses: string[]` → use those statuses verbatim.
3. Otherwise → collect distinct values across `filtered`, preserving first-seen order. Prepend an `"(unset)"` column if any entry has no value.
4. For each column, bucket entries by their value; entries with no value go in the `(unset)` column.

### Step 4 — `TaskCard.tsx` ([src/components/tasks/TaskCard.tsx](../../src/components/tasks/TaskCard.tsx))

Draggable card primitive:

```tsx
<TaskCard
  entry={entry}
  isSelected={...}
  onSelect={() => onSelectNote(entry)}
  locale={locale}
  dragHandleProps={...}    // optional — when used inside the board
/>
```

Layout:
- Top: title (single line, truncate)
- Below: chips for priority (P0/P1/P2/P3 colors via existing chip styles) + due-date badge (relative, e.g. "Due tomorrow")
- Bottom: assignee mini-row (count, or first 2 names truncated)

Degrades to title-only for non-task entries. Uses shadcn components throughout — no raw HTML for interactive bits.

### Step 5 — `TaskBoard.tsx` ([src/components/tasks/TaskBoard.tsx](../../src/components/tasks/TaskBoard.tsx))

```tsx
<TaskBoard
  view={view}
  filteredEntries={filteredEntries}
  allEntries={allEntries}
  selectedEntryPath={...}
  onSelectNote={...}
  onUpdateFrontmatter={(path, key, value) => Promise<void>}
  locale={locale}
/>
```

- Wrap in `DndContext` with `onDragEnd`.
- Render columns from `deriveBoardColumns(...)`.
- Each column is a `useDroppable` zone with a sticky header (column label + count).
- Each card is a `useDraggable` wrapper around `TaskCard`.
- `onDragEnd`: if `over.id` (column key) differs from card's current value → strip `note.` prefix from group-by field → call `onUpdateFrontmatter(entry.path, field, newValue ?? '')`. For `(unset)` column, write `null` (the existing save path handles property removal).
- Track `task_status_changed` PostHog event (no PII — just `{ from, to, property }`).
- Empty state: when filtered list is empty, show "No items in this view."

### Step 6 — Wire `display: board` into App.tsx ([src/App.tsx](../../src/App.tsx))

In the note-list column render branch (~line 1683), extend the existing PulseView ternary:

```tsx
{isPulseSelection ? (
  <PulseView ... />
) : selectedBoardView ? (
  <TaskBoard view={selectedBoardView} ... />
) : (
  <NoteList ... />
)}
```

`selectedBoardView` is `useMemo(() => findBoardViewForSelection(effectiveSelection, vault.views), [...])`. Returns the `ViewFile` only when its `definition.display === 'board'`.

### Step 7 — Localization

New keys in [src/lib/locales/en.json](../../src/lib/locales/en.json):

- `tasks.board.emptyView` — "No items in this view."
- `tasks.board.unsetColumn` — "(unset)"
- `tasks.board.columnCount` — "{{count}} item" / pluralized
- `tasks.board.dragHint` — "Drag a card to change its {{property}}"

Per the AGENTS.md l10n rule, run `pnpm l10n:translate` and `pnpm l10n:validate` before commit. (If lara-cli credentials are not present on this machine, seed the English placeholder in all non-en locales so the validator passes — same pattern as P2.)

### Step 8 — Unit tests

- `boardColumns.test.ts`:
  - Falls back to distinct values when no project binding
  - Uses project statuses verbatim when bound
  - Prepends `(unset)` only when needed
  - Buckets entries correctly
- `TaskCard.test.tsx`:
  - Renders title for any entry
  - Renders priority chip + due badge for task entries
  - Calls `onSelect` on click
- `TaskBoard.test.tsx`:
  - Renders one column per derived column
  - Empty filtered list → empty-state copy
  - Drop handler calls `onUpdateFrontmatter` with stripped field name + new value
  - Drop on `(unset)` column writes `null`

### Step 9 — Demo content

Drop a sample `views/board.yml` and a `tasks/` folder of task notes into [demo-vault-v2/](../../demo-vault-v2/) so the user can flip to the board view in the sidebar and see it work. Make sure `git status --short -- demo-vault-v2` is empty after — anything left over must be committed intentionally.

### Step 10 — QA + commit + push

Per [AGENTS.md §1c](../../AGENTS.md#1c-when-done):

- Native run: `pnpm tauri dev` → open the sample board view → drag a card to a different column → reload → verify the YAML on disk shows the new status.
- Frontend coverage gate: `pnpm test:coverage --silent` ≥ 70%.
- Rust gate: no Rust changes, but pre-push will still run `cargo test --lib`. Should pass unchanged.
- Playwright smoke: unchanged (drag tests stay in regression lane per the master plan).
- CodeScene: all new files at 10.0, touched files improved or held.
- Codacy: scan touched files with `.codacy/cli.sh`; fix any new High/Critical.
- Push: `git push origin main`. **Never `--no-verify`.**

## Out of scope

- Multi-view files (`views: [...]`) UI navigation — backend supports it from P3 but the sidebar list still renders one entry per file. Cosmetic; can ship later.
- Card sortable-within-column.
- Drag-resize of due dates (timeline phase, P6).
- WIP limits / column min-max counts.
- New-card affordance on the board (use the existing `New Task` command from P2).
- Field-mapping dropdowns for the `groupBy` property (the YAML is the source of truth in v1).
