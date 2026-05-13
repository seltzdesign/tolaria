---
status: in-progress
phase: P7.A
parent_plan: ./001-task-in-tolaria.md
depends_on:
  - ./001-p1-types.md
  - ./001-p4-board-view.md
---

# Plan 001 — P7.A — Sidebar Tasks rail entry

## Goal

Add a top-of-sidebar **Tasks** nav item that opens an aggregate "all open tasks across projects" view. Sibling to Inbox / All Notes / Archive. Pure frontend; no starter view files, no resource bundling.

This is the smallest valuable slice of the original master-plan P7 — embedded views in notes (`![[view.yml]]` transclusion + `this` context) stay queued as **P7.B** because that requires deep BlockNote schema work and is its own focused phase.

## Decisions (locked)

- **Filter, not saved view.** A new `kind: 'filter'` value of `'tasks'` (sibling to `'all'`, `'archived'`, `'inbox'`, `'pulse'`). No `.yml` to ship, no first-launch seeding, no backend changes. Matches how Inbox / All Notes work today.
- **Filter semantics.** Entry is included when:
  1. `entry.isA === 'task'`, AND
  2. `!entry.archived`, AND
  3. `entry.status` is not a terminal status — for v1 just `status !== 'Done'` (case-insensitive). Per-project `terminal_statuses` resolution is deferred; an explicit project filter remains the way to scope to one project's completion semantics.
- **Renders as a NoteList**, like the other filter selections. Users who want board / table / timeline create a saved view (we ship samples in `demo-vault-v2/views/`).
- **Sort:** reuse the existing NoteList default (modified desc). No per-filter override in v1.
- **Position in sidebar:** between Inbox and All Notes, gated on the same `showInbox` flag that controls Inbox visibility — when explicit organization is off, Tasks still shows (it doesn't depend on inbox semantics).

Actually re-reading: showInbox only hides Inbox when explicit organization is off. Tasks should be **always visible** regardless of that setting.

- **PostHog event:** none yet. Adding `sidebar_tasks_opened` is a P17 concern.

## Steps

### Step 1 — Extend `SidebarSelection` filter union

[src/types.ts](../../src/types.ts) — the `kind: 'filter'` discriminator already takes a `filter: 'all' | 'archived' | 'favorites' | 'inbox' | 'pulse'` union. Add `'tasks'`.

### Step 2 — Filter implementation in `noteListHelpers.ts`

Wire `'tasks'` into `filterByFilterType`:

```ts
if (filter === 'tasks') return entries.filter((e) => isOpenTask(e))
```

Define `isOpenTask(entry)` adjacent to the existing `isActive` helper:

```ts
function isOpenTask(entry: VaultEntry): boolean {
  if (entry.isA !== 'task') return false
  if (entry.archived) return false
  return (entry.status ?? '').toLowerCase() !== 'done'
}
```

### Step 3 — Sidebar nav item

[src/components/sidebar/SidebarTopNav.tsx](../../src/components/sidebar/SidebarTopNav.tsx) — add a `Tasks` `NavItem` between Inbox and All Notes, with a Phosphor `CheckSquare` icon (or similar). Add an `openTaskCount` prop computed in `App.tsx` like `inboxCount` / `activeCount`.

### Step 4 — Localization

`sidebar.nav.tasks` → "Tasks". Seed all 14 non-en locale catalogs with the English placeholder.

### Step 5 — Tests

- `noteListHelpers.test.ts`:
  - `filterEntries(..., { kind: 'filter', filter: 'tasks' })` returns only open task entries
  - Excludes archived tasks
  - Excludes tasks with `status: 'Done'` (case-insensitive)
  - Excludes non-task entries
- `Sidebar.test.tsx` (or `SidebarTopNav` if separate):
  - Renders the Tasks nav item
  - Clicking it calls `onSelect` with `{ kind: 'filter', filter: 'tasks' }`
  - Shows the open task count badge

### Step 6 — Docs

Update [docs/ARCHITECTURE.md](../ARCHITECTURE.md) and [docs/ABSTRACTIONS.md](../ABSTRACTIONS.md) with one short paragraph noting the built-in Tasks filter and its semantics.

### Step 7 — Commit + push

- `npx tsc --noEmit` clean
- `pnpm lint` clean
- `pnpm test --silent` — full FE suite green
- Push (pre-push runs full gate). **Never `--no-verify`.**

## Out of scope (deferred to P7.B)

- `![[view.yml]]` transclusion in BlockNote (requires custom block spec + markdown round-trip + `vault.views` threading through editor).
- `this` context in filters (only useful inside embedded views).
- Project starter template `![[project-board.yml]]` (depends on transclusion).
- Per-project `terminal_statuses` resolution in the global Tasks filter (would require walking task → project relationship). Use a saved view per project for accurate completion semantics.
