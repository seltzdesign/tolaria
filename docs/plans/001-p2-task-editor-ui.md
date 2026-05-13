# Plan 001 / P2 — Task Editor UI

> Parent plan: [001-task-in-tolaria.md](./001-task-in-tolaria.md)
> Previous phase: [001-p1-types.md](./001-p1-types.md)

## What we're doing (in plain language)

**The job:** when the user opens a note whose frontmatter says `type: task`, show them a task-shaped editor — a header with the task's status, priority, due date, start date, assignee, and project, plus the regular note body underneath. Editing any of those fields writes back to the YAML frontmatter on disk. Same for `type: project`, but with a project header (task folder, statuses, default view).

**Why it matters:** P1 made tasks legible to the Rust backend. P2 makes them visible and editable in the app. Until P2 lands, the only way to "use" a task is to hand-edit YAML — that's not a product, it's a parser. Every later phase (board view, table view, GitHub sync) depends on users being able to create and edit tasks through normal UI, not by typing dates into frontmatter blocks.

**What you'll see when it's done:**

- Open `task.md` from the vault → top of the editor shows a row of task property controls. Body editor below works exactly as before.
- Change the status pill from "Open" to "In progress" → the YAML `status:` line updates within the existing save debounce window.
- Click the "Due" date cell → shadcn `Calendar` opens in a `Popover`; pick a date, it lands in `due:` as `2026-05-20`.
- A new ⌘N-style "New Task" command in the command palette creates a task `.md` in the right folder, opens it, and parks the cursor in the title.
- The Inspector panel still shows everything it shows today; nothing is hidden or replaced. The new controls are additive.

**Roughly how long:** 2 focused days.

---

## Context (technical)

P1 introduced [`TaskView`](../../src-tauri/src/vault/task.rs) and [`ProjectView`](../../src-tauri/src/vault/task.rs) on the Rust side — typed accessors that read [`VaultEntry.properties`](../../src-tauri/src/types.ts) (scalars: `priority`, `due`, `start`, `completed`, `estimate`, `labels`, all `github_*` fields, plus project-specific `task_folder`, `statuses`, `terminal_statuses`, `default_view`) and [`VaultEntry.relationships`](../../src-tauri/src/types.ts) (wikilink fields: `project`, `assignee`, `blocked_by`). The frontend already receives these via the existing scan/cache pipeline — `VaultEntry.properties` and `VaultEntry.relationships` are populated for every note today, without any change required to the cache or watcher.

What's missing on the frontend:

- A type-aware editor router. [`EditorContentLayout`](../../src/components/editor-content/EditorContentLayout.tsx) currently branches on raw-mode vs rich-mode only. It needs a `type === 'task'` branch ahead of that, routing to a new `TaskEditor`.
- A `TaskEditor` component that renders a task-property header above the existing body editor — keeping the body editor exactly as it is today (no fork of BlockNote or RawEditor logic).
- A matching `ProjectEditor` component for `type: project`. Smaller surface than task — just the project metadata block.
- TypeScript task/project view types that mirror the Rust [`TaskView`](../../src-tauri/src/vault/task.rs) / [`ProjectView`](../../src-tauri/src/vault/task.rs) shape: pure read-derived views over `VaultEntry`, no separate store. Plus a [`DateOrDateTime`](../../src-tauri/src/vault/date_or_datetime.rs) parser/formatter mirroring the Rust helper.
- A `useTasks` hook that wraps the P1 Tauri commands ([`create_task`](../../src-tauri/src/commands/tasks.rs), [`create_project`](../../src-tauri/src/commands/tasks.rs)).
- A property writeback path. Status/priority/dates/labels/estimate flow through the existing frontmatter-mutation pipeline (`runFrontmatterAndApply` + `save_note_content`) — already used by `Inspector` for arbitrary properties. We do not introduce a parallel save path for tasks.

The existing [`Inspector`](../../src/components/Inspector.tsx) and [`PropertyValueCells`](../../src/components/PropertyValueCells.tsx) already render generic frontmatter properties with shadcn `Calendar`+`Popover` date pickers, status chips, etc. Some of those property cells will be the right reuse target for the task header; the rest are task-specific (status pill with project-defined statuses, project picker with vault-scoped autocomplete) and need new cells. We do not replace `Inspector`.

## Decisions specific to P2

1. **Two new components: `TaskEditor` and `ProjectEditor`.** Both wrap the existing editor body (whatever `EditorContentLayout` would have rendered) and add a property header. We do not fork the body editor; we wrap it. This keeps BlockNote/raw-editor swap logic in one place.
2. **Route in `EditorContentLayout`, not in `Editor.tsx`.** The branching is small: `if (entry.isA === 'task') return <TaskEditor entry={...}>{bodyEditor}</TaskEditor>`. Same for `project`. Anywhere higher and the wrapper has to re-implement Inspector + breadcrumb + window-mode logic; anywhere lower and we're editing the body editor itself.
3. **Property writeback reuses `runFrontmatterAndApply` + `save_note_content`.** No new mutation hook for tasks. The header cells emit `{ key, value }` updates that go through the same pipeline as `Inspector` property edits.
4. **Frontend `TaskView` / `ProjectView` are pure read-derived views.** Defined in `src/lib/tasks/taskView.ts` and `src/lib/tasks/projectView.ts`. They take a `VaultEntry` and expose typed getters (`status: string | null`, `due: DateOrDateTime | null`, `blockedBy: string[]`, etc.). They do not own state. They mirror Rust naming so the mental model is the same on both sides.
5. **`DateOrDateTime` is replicated on the frontend.** Same enum-of-two-cases shape as the Rust type: `{ kind: 'date'; date: string }` (ISO date, naive) or `{ kind: 'datetime'; iso: string }` (RFC 3339 with offset). Parser + formatter are 1:1 with the Rust implementation; the parsing rules in [`src-tauri/src/vault/date_or_datetime.rs`](../../src-tauri/src/vault/date_or_datetime.rs) are the contract.
6. **`useTasks` exposes `createTask` and `createProject`.** Thin wrappers around the P1 Tauri commands. No list/update methods — list is the existing entry scan; update is the property-writeback path above. This keeps the hook focused on the one thing the existing pipeline does not already cover: creation through a typed command.
7. **New "New Task" command palette entry.** Mirrors the existing "New Note" entry. When invoked, asks for a title (or auto-titles "Untitled task"), calls `createTask`, opens the new file, parks cursor in the title row.
8. **No board / table / timeline view in this phase.** Those are P4–P6 (Track B). P2 is editor-only.
9. **All form inputs are shadcn/ui or existing reusable Tolaria components.** Per [AGENTS.md §3](../../AGENTS.md#3-product-rules). Specifically: `Select` for status/priority, `Calendar` + `Popover` for date pickers, the existing wikilink autocomplete for assignee/project/blocked_by. No raw HTML form elements.
10. **PostHog event names locked here.** `task_created` (props: `has_project: bool`), `task_property_edited` (props: `property: 'status'|'priority'|'due'|'start'|'completed'|'assignee'|'project'|'labels'|'estimate'|'blocked_by'`), `project_created` (no props). No task titles, no values, no PII.

## Implementation breakdown

### Step 1 — Frontend `DateOrDateTime` (~1 hour)

New file: `src/lib/tasks/dateOrDateTime.ts`.

```ts
export type DateOrDateTime =
  | { kind: 'date'; date: string }        // 'YYYY-MM-DD'
  | { kind: 'datetime'; iso: string }     // RFC 3339 with offset, e.g. '2026-05-20T14:00:00+02:00'

export function parseDateOrDateTime(raw: string): DateOrDateTime | null
export function formatDateOrDateTime(v: DateOrDateTime): string
export function toNaiveDate(v: DateOrDateTime): string  // always 'YYYY-MM-DD'
export function hasTime(v: DateOrDateTime): boolean
```

Tests cover the same cases as the Rust [`date_or_datetime`](../../src-tauri/src/vault/date_or_datetime.rs) test file — date-only, datetime with `Z`, datetime with explicit offset, whitespace trimming, malformed input, `to_naive_date` truncation.

### Step 2 — Frontend `TaskView` and `ProjectView` (~2 hours)

New files: `src/lib/tasks/taskView.ts`, `src/lib/tasks/projectView.ts`.

```ts
export class TaskView {
  constructor(private entry: VaultEntry) {}
  get status(): string | null
  get priority(): string | null
  get due(): DateOrDateTime | null
  get start(): DateOrDateTime | null
  get completed(): DateOrDateTime | null
  get estimate(): number | null
  get labels(): string[]
  get project(): string | null         // wikilink target
  get assignees(): string[]            // wikilink targets
  get blockedBy(): string[]            // wikilink targets
  get githubSyncStatus(): string | null
  get githubItemNodeId(): string | null
  // ...other github_* fields
}

export class ProjectView { /* mirror of Rust ProjectView */ }

export function asTask(entry: VaultEntry): TaskView | null  // returns null if not a task
export function asProject(entry: VaultEntry): ProjectView | null
```

Tests: parse a hand-crafted `VaultEntry` shaped exactly like P1's Rust fixtures (same property keys and values) and assert each getter returns the right value.

### Step 3 — `useTasks` hook (~30 min)

New file: `src/hooks/useTasks.ts`.

```ts
export function useTasks() {
  const createTask = useCallback(async (folder: string, title: string, project?: string) => {
    return invoke<CreateNoteResult>('create_task', { vaultPath, folder, title, project })
  }, [vaultPath])

  const createProject = useCallback(async (folder: string, title: string) => {
    return invoke<CreateNoteResult>('create_project', { vaultPath, folder, title })
  }, [vaultPath])

  return { createTask, createProject }
}
```

Mock branch for `mock-tauri` follows the same pattern as `useNoteCreation`. Tests verify the invoke args.

### Step 4 — `TaskHeader` and `ProjectHeader` cells (~4 hours)

New files: `src/components/tasks/TaskHeader.tsx`, `src/components/tasks/ProjectHeader.tsx`, plus a `src/components/tasks/cells/` directory with one cell per field type.

Cells:

- `StatusPillCell` — `Select` populated from the project's `statuses` list if the task has a `project` relationship pointing to a project note in the same vault; otherwise a free-text input with "Open / In progress / Done" suggestions.
- `PriorityCell` — `Select` with `P0 / P1 / P2 / P3 / —`.
- `DateCell` — `Popover` + shadcn `Calendar` for the date portion. Time-of-day input is deferred; the cell preserves any time component already on disk (round-trips datetime values via `formatDateOrDateTime`).
- `EstimateCell` — shadcn `Input type="number"`.
- `LabelsCell` — chip list with type-to-add; reuse [`TagsInput`](../../src/components/ui/TagsInput.tsx) if available, else a new minimal chip input.
- `AssigneeCell` and `ProjectCell` — wikilink autocomplete reused from the existing editor's wikilink suggestion menu.
- `BlockedByCell` — multi-value wikilink picker (chip list of wikilinks). Editing this cell triggers the Rust `has_blocked_by_cycle` check by passing the updated `blocked_by` array through `save_note_content`; if the response carries a circular-dependency warning we surface a toast (warning style, not error — the save still lands).

Header layout: a single `<header>` row with cell groups (Status • Priority • Due • Start • Assignees • Project • Labels • Estimate • Blocked by). Wraps responsively at narrow widths. Light-mode design per [AGENTS.md §1b](../../AGENTS.md#1b-implement) using the visual language from [ui-design.pen](../../ui-design.pen).

Each cell emits `(propertyKey, newValue) => void`; the parent `TaskHeader` passes them through to a shared `useTaskPropertyMutator(entry)` that funnels into `runFrontmatterAndApply` + `save_note_content`. PostHog `task_property_edited` fires on commit (after debounce, on the same tick as the save).

### Step 5 — `TaskEditor` and `ProjectEditor` wrappers (~2 hours)

New files: `src/components/tasks/TaskEditor.tsx`, `src/components/tasks/ProjectEditor.tsx`.

```tsx
export function TaskEditor({ entry, children }: { entry: VaultEntry; children: ReactNode }) {
  const task = asTask(entry)
  if (!task) return <>{children}</>
  return (
    <div className="task-editor">
      <TaskHeader entry={entry} task={task} />
      {children}
    </div>
  )
}
```

`ProjectEditor` is the same shape with `asProject` + `ProjectHeader`.

### Step 6 — Route in `EditorContentLayout` (~30 min)

Add a wrapper branch at the top of the existing body-render logic:

```tsx
const body = /* existing SingleEditorView | RawEditorView selection */
if (entry?.isA === 'task') return <TaskEditor entry={entry}>{body}</TaskEditor>
if (entry?.isA === 'project') return <ProjectEditor entry={entry}>{body}</ProjectEditor>
return body
```

Don't move any of the existing logic. Just wrap.

### Step 7 — Command-palette "New Task" and "New Project" (~1 hour)

Add entries to the command palette source ([`src/lib/commands/index.ts`](../../src/lib/commands/index.ts) or the equivalent registry). Each entry:

- Prompts for title (existing input dialog component, same as "New Note").
- Calls `createTask` / `createProject` with `folder = activeFolder ?? ''`.
- Awaits the result, opens the file via existing open-note flow, focuses the title.
- Fires `task_created` / `project_created` PostHog event.

### Step 8 — Tests (~3 hours)

Vitest:

- `dateOrDateTime.test.ts` — mirrors Rust test cases.
- `taskView.test.ts` — VaultEntry fixture → asserts every getter.
- `projectView.test.ts` — same.
- `useTasks.test.ts` — mock invoke, assert arg shape for both commands.
- `TaskHeader.test.tsx` — renders all cells, change events emit expected mutations.
- `TaskEditor.test.tsx` — when `entry.isA === 'task'`, header renders above children; when not, children render alone.

Playwright (`tests/smoke/task-editor.spec.ts`, `@smoke`):

- Create a task via command palette → editor opens with empty header + body.
- Type title, set status, set due → reload vault → properties persist on disk (read via `git diff` against the new file).
- One regression scenario: open a non-task note → no task header rendered.

### Step 9 — l10n + PostHog + docs (~1.5 hours)

- All new UI strings into [`src/lib/locales/en.json`](../../src/lib/locales/en.json) under a new `tasks.` namespace. Run `pnpm l10n:translate` and `pnpm l10n:validate`.
- PostHog events as defined in decision 10. Add to whatever the project's PostHog event registry is (look for `posthog.capture(` usage).
- [docs/ARCHITECTURE.md](../ARCHITECTURE.md): add a one-paragraph section under "Notes & Editor" describing the type-aware editor route. [docs/ABSTRACTIONS.md](../ABSTRACTIONS.md): add `TaskView` / `ProjectView` / `DateOrDateTime` to the Frontend abstractions table.

### Step 10 — CodeScene + Codacy + push (~1 hour)

Per [AGENTS.md §2](../../AGENTS.md#2-development-process):

- File-level CodeScene check on every touched/new file. New scorable files must hit 10.0.
- Codacy scan on every touched file (MCP if available, else `.codacy/cli.sh`). No new Critical/High.
- Coverage: frontend ≥70%, Rust unchanged (P2 is FE-only). Rust coverage gate runs anyway on push.
- Push through the full pre-push gate.

## Acceptance criteria

1. Opening a `.md` whose frontmatter has `type: task` renders the `TaskEditor` wrapper. Header cells display each task field (or "—" when null).
2. Editing any cell writes the corresponding frontmatter key within the existing save debounce window.
3. Setting a `due` date round-trips: write a date → reload the note from disk → cell shows the same date.
4. Opening a `.md` whose frontmatter has `type: project` renders `ProjectEditor` with the project header.
5. Command palette has "New Task" and "New Project" entries; both create a typed note in the active folder, open it, focus the title.
6. `blocked_by` edit that introduces a cycle surfaces a non-blocking warning toast; the save still lands (per P1's "best-effort, not exhaustive" cycle detection).
7. Coverage ≥70% on FE; CodeScene Hotspot + Average pass `.codescene-thresholds`; no new Critical/High Codacy findings.
8. All new UI copy lives in `en.json` and `pnpm l10n:validate` passes.
9. `task_created`, `task_property_edited`, `project_created` PostHog events fire with the documented props.

## Out of scope for P2

- Board / table / timeline views (P4–P6).
- Embedded views in notes (P7).
- GitHub Projects sync of any kind (P8+).
- Time-of-day editing in `DateCell` (date is sufficient for v1; existing datetime values round-trip but the cell does not let you change the time component yet).
- Bulk operations across many tasks.
- A "tasks home" dashboard (P7).
- Migration tooling for tasks created before this phase — there are none; this is the first product surface.
