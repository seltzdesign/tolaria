---
status: in-progress
phase: P6
parent_plan: ./001-task-in-tolaria.md
depends_on:
  - ./001-p3-view-engine.md
  - ./001-p4-board-view.md
  - ./001-p5-table-view.md
---

# Plan 001 — P6 — Timeline (gantt) view

## Goal

When a saved view declares `display: timeline`, render a horizontal swimlane gantt in the shared `.app__view-canvas` slot. Each row is one swimlane (`groupBy.property`, defaulting to `assignee`), each bar spans a task's `start` → `due`. Bars can be dragged on either edge to resize start/due, or in the middle to move both, with snap-to-day. On `pointerup`, the new dates write back through the same `onUpdateFrontmatter` pipeline P4 uses.

Per the [master plan](./001-task-in-tolaria.md#p6--timeline-view-3-days), this is the v1 timeline phase. ~1–2 day scope; the bigger 3-day estimate in the master plan reserves time for virtualization and richer drag affordances which are deferred below.

## Decisions (locked)

- **UI slot:** same `.app__view-canvas` as board and table (one more `kind` in `selectedCanvasView`).
- **SVG hand-rolled.** No external charting library. The DOM cost of bars + grid for ≤500 tasks fits comfortably without virtualization.
- **Date range:**
  - If any visible entry has a `start` or `due`, span `min(start, due) - 3 days` to `max(start, due) + 3 days`.
  - If no dates at all, span `today - 7 days` to `today + 30 days` and render an empty-state hint.
  - Day width: **40px** at zoom 1 (no zoom control in v1).
- **Swimlane source:**
  - `groupBy.property` (default `assignee`).
  - For relationship fields (`assignee`, `project`, `blocked_by`), each entry contributes a row per linked target; pure scalar fields use a single value.
  - Entries without a value for the field go into an `(unset)` swimlane.
  - Swimlane order: alphabetical, `(unset)` last.
- **Bar shape:**
  - Tasks with both `start` and `due` → bar from `start` to `due` (min one day wide).
  - Tasks with only `due` → single-day bar on `due`.
  - Tasks with only `start` → single-day bar on `start`.
  - Tasks with neither → omitted from the timeline (with a footer count saying "N tasks without dates"). No phantom bars.
- **Drag-resize:**
  - Three drag zones per bar — left edge, middle, right edge. Edge handles ~8px wide.
  - Snap to day on pointermove.
  - On `pointerup`, call `onUpdateFrontmatter(entry.path, key, newDateISO)` once for each changed date. Move = two updates (start AND due); resize-end = one update (`due`); resize-start = one update (`start`).
  - PostHog event `task_dates_changed` with `{ field }` (just `start` / `due` / `both`) — no PII, no values.
  - For accessibility v1, drag is mouse/touch only. Keyboard date edits go through the existing date cells in the editor on the right.
- **Click without drag** (mouse moved <4px) → opens the task via `onSelectNote`. Same convention as the board.
- **Today line:** a thin vertical accent rule at `today`.
- **Sticky x-axis:** the date scale header sticks to the top of the scroll container; the swimlane labels stick to the left edge.
- **Empty state:** `tasks.timeline.emptyView` copy mirrors board / table.

## Steps

### Step 1 — Pure date helpers in `src/lib/tasks/timelineLayout.ts`

```ts
export interface DateRange { startMs: number; endMs: number; days: number }
export interface BarLayout { entry: VaultEntry; startMs: number; endMs: number; xPx: number; widthPx: number; lane: string }
export interface LaneGroup { lane: string; label: string; isUnset: boolean; bars: BarLayout[] }

export function dateRangeFor(entries: VaultEntry[], today: Date): DateRange
export function layoutBars(entries: VaultEntry[], range: DateRange, groupBy: ViewGroupBy, dayWidthPx: number): LaneGroup[]
export function pixelToDayOffset(px: number, dayWidthPx: number): number   // rounds to nearest day
export function isoDateForOffset(range: DateRange, dayOffset: number): string  // YYYY-MM-DD
```

Pure, side-effect-free, fully testable.

### Step 2 — `src/components/tasks/TaskTimeline.tsx`

- Receives `view`, `filteredEntries`, `allEntries`, `selectedEntryPath`, `onSelectNote`, `onUpdateFrontmatter`, `locale`.
- Computes `dateRangeFor(filteredEntries, new Date())` once per `filteredEntries`.
- Computes `layoutBars(filteredEntries, range, groupBy, DAY_WIDTH)`.
- Renders:
  - `<header>` row with day-by-day dates (week starts highlighted)
  - left column with swimlane labels
  - SVG canvas with one `<rect>` per bar + a today vertical line
- Each bar wraps three `<rect>` invisible drag handles (or a single `<rect>` with edge-zone hit-testing) + a visible body `<rect>`.
- Plain pointer events; no dnd-kit (drag-resize is rect math, not list reordering).
- On `pointerup`, dispatches the updates and emits `task_dates_changed`.

### Step 3 — Wire into `App.tsx`

Extend the existing `selectedCanvasView` memo to include `kind: 'timeline'`. JSX branches on `.kind` and renders `<TaskTimeline ... />`.

### Step 4 — Localization + sample view

- New keys in `en.json`:
  - `tasks.timeline.emptyView`
  - `tasks.timeline.unsetLane`
  - `tasks.timeline.noDatesFooter` — `"{count} task without dates"` / pluralized
  - `tasks.timeline.today`
- Seed all non-en catalogs with English placeholders.
- New demo view `demo-vault-v2/views/q2-launch-timeline.yml` filtered to Q2 Launch tasks.

### Step 5 — Tests

- `timelineLayout.test.ts`:
  - `dateRangeFor` returns the today-centered fallback when no dates
  - Bars laid out at the correct x/width for known date ranges
  - `pixelToDayOffset` snaps correctly
  - Lane grouping picks up project / assignee / status as group-by
- `TaskTimeline.test.tsx`:
  - Renders empty state when no entries
  - Renders one bar per dated entry, none for date-less entries (footer count shown instead)
  - Click without movement triggers `onSelectNote`
  - Pointerup after edge drag fires `onUpdateFrontmatter` with the new ISO date
  - Pointerup after middle drag fires two `onUpdateFrontmatter` calls (start + due)

### Step 6 — Docs

Update [docs/ARCHITECTURE.md](../ARCHITECTURE.md) and [docs/ABSTRACTIONS.md](../ABSTRACTIONS.md) with a Timeline section under Board / Table.

### Step 7 — Commit + push

- `npx tsc --noEmit` clean
- `pnpm lint` clean
- `pnpm test --silent` — full FE suite green
- Push (pre-push runs Playwright smoke + Rust + CodeScene gate). **Never `--no-verify`.**

## Out of scope

- Virtualization for >500 tasks (the master plan threshold; we'll revisit when a real timeline crosses that).
- Zoom controls (week / month / quarter density toggles).
- Keyboard-driven drag for accessibility (use the date cells in the editor for now).
- Dependency arrows between blocked-by tasks.
- Drag a bar between swimlanes to reassign (would need a separate `onUpdateFrontmatter` for the group-by field; defer to v1.5 once we see how it feels).
- Pan via spacebar / middle-mouse. Plain horizontal scroll is fine for v1.
