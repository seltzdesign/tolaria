# Plan 001 / P1 — Task & Project Types

> Parent plan: [001-task-in-tolaria.md](./001-task-in-tolaria.md)
> Phase 0: [001-p0-foundations.md](./001-p0-foundations.md)

## What we're doing (in plain language)

**The job:** teach Tolaria's Rust backend to recognize when a markdown file is a task or a project, and how to read its specific fields (due dates, priority, dependencies, etc.) as proper typed values instead of raw YAML strings.

**Why it matters:** every Track B view (board, table, timeline) and the entire GitHub Projects bridge need to read tasks the same way. If we don't centralize that once, we'll re-implement it five times across five components and they'll drift apart on edge cases — "What does an empty `due` mean? What about a malformed date? Is `blocked_by: [[X]]` a single string or a list?" This phase is the one place those questions get answered.

**What you'll see when it's done:** nothing visible yet. This is backend plumbing — no UI changes. The proof is in tests and in being able to drop a `.md` file with all the v1 task fields into the vault and have the parser accept it correctly. You'll also be able to create a new task via a Tauri command (no UI button yet — that's P2), which writes a properly-formatted `.md` file to disk.

**Roughly how long:** 2–3 focused days.

---

## Context (technical)

[ADR 0115](../adr/0115-tasks-and-projects-as-typed-notes.md) locked the task and project frontmatter schemas in P0. Now we make Rust honor them.

The existing frontmatter parser in [src-tauri/src/vault/frontmatter.rs](../../src-tauri/src/vault/frontmatter.rs) is a single `Frontmatter` struct deserialized via `serde` with a fixed set of known fields. The `type` field already exists (it reads from `type:` in YAML, with aliases `Is A` and `is_a` for legacy notes). `status` is already a `StringOrList` — free-form per [ADR 0115 §5](../adr/0115-tasks-and-projects-as-typed-notes.md), no enum constraint.

What's missing:

- Task-specific fields (`priority`, `due`, `start`, `completed`, `assignee`, `project`, `blocked_by`, `labels`, `estimate`, plus the seven `github_*` bridge fields).
- Project-specific fields (`task_folder`, `statuses`, `terminal_statuses`, `default_view`, the GitHub binding block, the field-mapping block).
- A type that handles "ISO date `YYYY-MM-DD` OR RFC 3339 datetime `YYYY-MM-DDTHH:MM:SS±HH:MM`" — needed for `due`, `start`, `completed` ([ADR 0115 §8](../adr/0115-tasks-and-projects-as-typed-notes.md)).
- Wikilink-list parsing for `assignee`, `blocked_by`, and `project` (extract the target from `[[Title]]` or `[[Title|Alias]]` syntax).
- Circular dependency detection for `blocked_by` ([ADR 0115 §4](../adr/0115-tasks-and-projects-as-typed-notes.md)).
- A `create_task` Tauri command — used by P2's UI and useful for QA right now.
- Starter type documents at `task.md` and `project.md` ([ADR 0096](../adr/0096-root-created-type-documents.md)).

The existing parser shape is the model to follow — see `StringOrList` and `deserialize_bool_or_string` in [frontmatter.rs](../../src-tauri/src/vault/frontmatter.rs). The new typed fields use the same pattern.

## Decisions specific to P1

Most decisions were already locked in P0's ADRs. The few that remain are implementation-level:

1. **Reuse the existing generic property/relationship extraction; don't extend the `Frontmatter` struct.** A first pass of the plan called for adding ~20 typed fields to `Frontmatter`. After reading [src-tauri/src/vault/frontmatter.rs](../../src-tauri/src/vault/frontmatter.rs) and [parsing.rs](../../src-tauri/src/vault/parsing.rs), the existing pipeline already captures every task/project field automatically: `extract_properties` puts scalars (`priority`, `due`, `estimate`, `labels`, `github_*`, `task_folder`, `statuses`, `terminal_statuses`, etc.) into `VaultEntry.properties`, and `extract_relationships` puts wikilink-containing fields (`project`, `assignee`, `blocked_by`) into `VaultEntry.relationships`. Typed access happens at read time via `TaskView` / `ProjectView`, not at deserialize time. This is the same model existing custom frontmatter has used since [ADR 0040](../adr/0040-custom-views-yml-filter-engine.md).
2. **Starter type documents are seeded lazily.** The `task.md` and `project.md` type docs are not created on app open or vault open. They appear on first invocation of `create_task` (or its sibling `create_project`) — if the vault has no `task.md` at root, one is materialized from `src-tauri/resources/starter-types/task.md` in the same call. This avoids proactively polluting vaults that don't use the feature.
3. **`create_task` is minimal in v1.** Parameters: `folder` (vault-relative), `title`, optional `project` wikilink. All other fields are filled by the editor (P2). Returns the new file path and any warnings (e.g., the lazy-seeded type doc).
4. **Circular dependency check is best-effort, not exhaustive.** v1 walks the `blocked_by` graph up to depth 32 looking for a cycle that includes the current task. If found, returns a `CircularDependencyWarning` alongside the successful save. We do NOT walk the full graph or detect cycles between unrelated tasks — that's vault-scope work; v1 only cares about cycles touching the task being saved.
5. **Wikilink target extraction follows [ADR 0035](../adr/0035-path-suffix-wikilink-resolution.md).** For `[[Title]]` → target is `Title`. For `[[Title|Alias]]` → target is `Title`. The existing `extract_outgoing_links` helper in [parsing.rs](../../src-tauri/src/vault/parsing.rs) already does this; reuse it.
6. **The `is_a:` → `type:` find-and-replace in the parent plan** happens in the first commit of this phase. Done.

## Implementation breakdown

### Step 1 — Parent plan cleanup (~15 min)

- Replace `is_a:` with `type:` in [001-task-in-tolaria.md](./001-task-in-tolaria.md) YAML examples, update the project schema to match [ADR 0115 §3](../adr/0115-tasks-and-projects-as-typed-notes.md), add a pointer to the ADRs as authoritative.
- Done in the same commit as the P1 plan revision below.

### Step 2 — DateOrDateTime parse/format helper (~1.5 hours)

New file: `src-tauri/src/vault/date_or_datetime.rs`.

```rust
pub enum DateOrDateTime {
    Date(chrono::NaiveDate),                          // 2026-05-20
    DateTime(chrono::DateTime<chrono::FixedOffset>),  // 2026-05-20T14:00:00+02:00
}

impl DateOrDateTime {
    pub fn parse(s: &str) -> Result<Self, ParseError>;
    pub fn to_storage_string(&self) -> String;       // round-trippable form for writing back
    pub fn to_naive_date(&self) -> chrono::NaiveDate; // for day-granularity filters
}
```

- `parse` accepts `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM:SSZ`, `YYYY-MM-DDTHH:MM:SS±HH:MM`, and `YYYY-MM-DDTHH:MM:SS` (treated as system local per [ADR 0115 §8](../adr/0115-tasks-and-projects-as-typed-notes.md)).
- No serde integration. Date fields live in `VaultEntry.properties` as `String` and we parse on read in `TaskView`. Writes go through `to_storage_string()`.
- Tests: each accepted shape, invalid garbage, naive-datetime local-tz fallback round-trip.

### Step 3 — Typed views on `VaultEntry` (~2 hours)

Edit: [src-tauri/src/vault/entry.rs](../../src-tauri/src/vault/entry.rs).

```rust
impl VaultEntry {
    pub fn is_task(&self) -> bool { /* is_a.as_deref() == Some("task") */ }
    pub fn is_project(&self) -> bool { /* is_a.as_deref() == Some("project") */ }
    pub fn as_task(&self) -> Option<TaskView<'_>> { /* None if !is_task() */ }
    pub fn as_project(&self) -> Option<ProjectView<'_>> { /* None if !is_project() */ }
}

pub struct TaskView<'a>(&'a VaultEntry);
pub struct ProjectView<'a>(&'a VaultEntry);
```

`TaskView` and `ProjectView` are zero-cost borrow wrappers. Accessors read on demand from the entry's `properties` and `relationships` maps and parse strings into typed values:

```rust
impl<'a> TaskView<'a> {
    pub fn priority(&self) -> Option<&str>;
    pub fn due(&self) -> Option<DateOrDateTime>;        // parses from properties["due"]
    pub fn start(&self) -> Option<DateOrDateTime>;
    pub fn completed(&self) -> Option<DateOrDateTime>;
    pub fn estimate(&self) -> Option<f64>;
    pub fn labels(&self) -> Vec<&str>;
    pub fn project(&self) -> Option<String>;            // extracts wikilink target
    pub fn assignees(&self) -> Vec<String>;             // extracts wikilink targets
    pub fn blocked_by(&self) -> Vec<String>;            // extracts wikilink targets
    pub fn github_sync_status(&self) -> Option<&str>;
    pub fn github_item_node_id(&self) -> Option<&str>;
    // ...etc. for the rest of the github_* fields
}

impl<'a> ProjectView<'a> {
    pub fn task_folder(&self) -> Option<&str>;
    pub fn statuses(&self) -> Vec<&str>;
    pub fn terminal_statuses(&self) -> Vec<&str>;       // defaults to statuses.last() if not set
    pub fn default_view(&self) -> Option<&str>;
    pub fn sync_enabled(&self) -> bool;
    pub fn sync_interval_minutes(&self) -> u32;          // default 5
    pub fn link_to_issues(&self) -> bool;
    pub fn github_project_node_id(&self) -> Option<&str>;
    // ...etc.
}
```

Wikilink target extraction reuses `extract_outgoing_links` semantics from [parsing.rs](../../src-tauri/src/vault/parsing.rs) — extract a small helper (`wikilink_target(s) -> Option<String>`) callable on a single bracketed string.

Tests: build a `VaultEntry` from a task `.md` fixture in code, exercise every accessor, verify `as_project()` returns `None` for tasks and vice versa.

### Step 4 — Circular dependency detection (~2 hours)

New module: `src-tauri/src/vault/task_graph.rs`.

```rust
pub fn has_blocked_by_cycle(
    task_path: &Path,
    blocked_by: &[WikilinkTarget],
    vault: &VaultCache,
    max_depth: u32,  // default 32
) -> bool
```

- DFS from the task's `blocked_by` list, following each target's wikilink to its `.md` file (via existing wikilink resolver), reading its own `blocked_by`, recursing.
- Returns `true` if any path leads back to the original task.
- Caller decides what to do with the result; `create_task` and the eventual save path use it to produce a non-blocking warning.

Tests: linear chain (no cycle), 2-node cycle, 3-node cycle, self-loop, depth-exceeded (deeper than 32 — returns `false` to avoid stack overflow), broken wikilink mid-chain (treated as terminal, no cycle).

### Step 5 — Starter type documents (~30 min)

New files:

- `src-tauri/resources/starter-types/task.md`
- `src-tauri/resources/starter-types/project.md`

Content per [ADR 0096](../adr/0096-root-created-type-documents.md) and [ADR 0115 §1](../adr/0115-tasks-and-projects-as-typed-notes.md). Frontmatter:

```yaml
---
type: Type
icon: check-circle    # task; project gets `folder-kanban`
color: blue           # task; project gets `amber`
sidebar label: Tasks  # task; project gets `Projects`
---
```

Body: brief plain-language explanation of what the type is and what fields are supported. The body becomes the user's documentation when they open the file.

Bundling the resource: add `tauri.conf.json` resource path (or follow whatever pattern existing starter content uses — check `vault/getting_started.rs` for precedent).

### Step 6 — `create_task` Tauri command (~3 hours)

New file: `src-tauri/src/commands/tasks.rs`.

```rust
#[tauri::command]
pub async fn create_task(
    vault_path: String,
    folder: String,
    title: String,
    project: Option<String>,
) -> Result<CreateTaskResult, String>

pub struct CreateTaskResult {
    pub path: String,
    pub warnings: Vec<String>,  // e.g., "seeded task.md type document at vault root"
}
```

Behavior:

1. Lazy-seed `<vault>/task.md` from `src-tauri/resources/starter-types/task.md` if missing. Add to warnings.
2. Compute filename from `title` via existing title→filename rules in [src-tauri/src/vault/filename_rules.rs](../../src-tauri/src/vault/filename_rules.rs).
3. Resolve collisions via `(2)`, `(3)` suffix per [ADR 0007](../adr/0007-title-filename-sync.md).
4. Write the `.md` file via the existing crash-safe write path ([ADR 0075](../adr/0075-crash-safe-note-rename-transactions.md)).
5. Frontmatter: `type: task`, `title: <provided>`, `project: [[<resolved>]]` if provided. Status defaults to first entry of the project's `statuses` (or `"Not started"` if standalone). No other fields populated — the editor (P2) handles those.
6. Return path + warnings.

Register the command in [src-tauri/src/commands/mod.rs](../../src-tauri/src/commands/mod.rs) and add to the invoke handler list in [src-tauri/src/lib.rs](../../src-tauri/src/lib.rs).

A matching `create_project` follows the same shape but for project notes — same step, ~30 min addition.

Tests: integration test creating a task in a temp vault, verifying file contents and that `task.md` gets seeded on first call only.

### Step 7 — Wire-up & smoke (~1 hour)

- `cargo build` clean
- `cargo test` green (new tests + existing tests still pass)
- `cargo llvm-cov` ≥ 85% on touched files
- Manual smoke: `pnpm tauri dev`, open devtools console, call the new commands via `__TAURI__.invoke()`, verify files created and parsed correctly when reopened.

## Test plan

Rust unit tests live alongside their source files (existing convention). New test cases by area:

| Area | Tests |
|---|---|
| `date_or_datetime.rs` | `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM:SSZ`, `YYYY-MM-DDTHH:MM:SS±HH:MM`, naive-datetime-fallback, invalid garbage, round-trip via `to_storage_string` |
| `entry.rs` (TaskView/ProjectView) | `as_task()` returns Some for `type: task`, None for `type: project`, None for plain note; every accessor returns the expected value from a fixture entry; wikilink fields extract targets from `[[X]]` and `[[X\|Alias]]` |
| `task_graph.rs` | linear chain, 2-cycle, 3-cycle, self-loop, depth-exceeded, broken wikilink mid-chain |
| `commands/tasks.rs` | integration: create_task in fresh vault seeds task.md, second create_task does NOT re-seed, collision suffix appended |

Coverage target: 85% on every new file (CodeScene gate is the floor; aim higher).

## Reusable existing utilities

- `StringOrList` and `deserialize_bool_or_string` in [frontmatter.rs](../../src-tauri/src/vault/frontmatter.rs) — patterns to copy for new typed fields.
- [filename_rules.rs](../../src-tauri/src/vault/filename_rules.rs) — title→filename for `create_task`.
- Existing wikilink target extraction (search for `[[` parsing in [parsing.rs](../../src-tauri/src/vault/parsing.rs) before writing a new one).
- Crash-safe write/rename pattern from [ADR 0075](../adr/0075-crash-safe-note-rename-transactions.md) implementation in [rename_transaction.rs](../../src-tauri/src/vault/rename_transaction.rs).
- Vault root + type document seeding pattern from [vault/getting_started.rs](../../src-tauri/src/vault/getting_started.rs) — examine first before writing the lazy-seed logic.

## Verification

End-to-end check that P1 is done:

1. **Cargo green:** `cd src-tauri && cargo test && cargo llvm-cov --fail-under-lines 85`.
2. **CodeScene:** every new file scores 10.0; every touched file's score holds or improves. Check with `mcp__codescene__code_health_score` per [AGENTS.md §2 code health](../../AGENTS.md).
3. **Manual parser smoke:** hand-write a complete task `.md` in `demo-vault-v2/` per the ADR 0115 §2 schema (all fields populated including datetime `due` and `blocked_by` with a wikilink) → reopen the vault → verify devtools shows the parsed task with all typed fields.
4. **Manual `create_task` smoke:** call `create_task` via devtools `__TAURI__.invoke()` in a fresh demo-vault — verify the file lands, `task.md` gets seeded at vault root, and a second call doesn't re-seed.
5. **Circular dep manual:** hand-write task A with `blocked_by: [[B]]` and task B with `blocked_by: [[A]]` → call `create_task` for a new task C with `blocked_by: [[A]]` → verify warning surfaces (currently to stdout, since no UI).
6. **Demo vault hygiene:** `git status --short -- demo-vault demo-vault-v2` clean before push.
7. **Codacy:** `.codacy/cli.sh analyze` on every touched Rust file, fix any new Critical/High.

## Out of scope for P1

- Any frontend / React / TS code. Strictly Rust backend in this phase.
- View engine extensions (`file.X` fields, `display: board`, multi-view files). Those are P3.
- The task editor UI (P2).
- Any GitHub bridge code (P8+).
- Localization. No user-visible strings change in this phase (warnings are dev-stdout-only in v1).
- PostHog events. The user-visible behavior begins in P2.
- Playwright tests. No UI to test.
- Updating the parent `001-task-in-tolaria.md` plan beyond the `is_a`→`type` fix in Step 1.

## After P1 — Phase 2 ready-state

When P1 is merged on `main`, P2 (task editor UI) starts with:

- `VaultEntry.as_task()` returning a fully-typed view it can render against.
- A `create_task` Tauri command it can wire to a "New Task" button.
- Round-trip-safe serialization so editor saves don't mangle the frontmatter.
- A circular-dependency warning channel ready to wire to a toast / banner.

P3 (view engine extensions) is unblocked in parallel — it doesn't strictly need P1's typed views to start, but having them makes the board view's data access trivial.

## Estimated effort

| Step | Hours |
|---|---|
| 1. Parent plan `is_a`→`type` cleanup + P1 plan revision | 0.5 |
| 2. DateOrDateTime parse/format helper | 1.5 |
| 3. TaskView / ProjectView accessors | 2 |
| 4. Circular dependency detection | 2 |
| 5. Starter type documents | 0.5 |
| 6. `create_task` + `create_project` commands | 3 |
| 7. Wire-up, tests, smoke | 1 |
| **Total** | **~10.5 hours** |

About **1.5 focused days**. Smaller than the original ~15h estimate because we discovered the existing generic property/relationship extraction already captures every task/project field — no `Frontmatter` struct extension and no `WikilinkList` serde type needed (decision 1).

Each step is a separate commit on `main` with conventional `feat:` / `refactor:` / `test:` prefixes per [AGENTS.md §1b](../../AGENTS.md). Direct push to `main` per [ADR 0021](../adr/0021-push-to-main-workflow.md). Pre-push runs the full check suite; expect it to pass clean since this phase produces well-tested, low-complexity additions to an existing pattern.
