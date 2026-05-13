# Plan 001 / P3 — View Engine Extensions

> Parent plan: [001-task-in-tolaria.md](./001-task-in-tolaria.md)
> Previous phase: [001-p2-task-editor-ui.md](./001-p2-task-editor-ui.md)

## What we're doing (in plain language)

**The job:** make the existing saved-view engine understand three new things:

1. **What kind of view it is** — a `display:` field on each view that says `list`, `table`, `board`, `timeline`, or `cards`. P3 only adds the field and the parser; the new renderers come in P4–P6.
2. **Field namespaces** — let view files name fields as `note.priority` (frontmatter property), `file.name` / `file.path` / `file.folder` / `file.ext` / `file.size` / `file.ctime` / `file.mtime` / `file.tags` (filesystem / entry metadata), or `formula.X` (reserved, errors with "v2 feature"). Bare names like `priority` keep working — they resolve to `note.priority`.
3. **Multi-view files** — let one `.yml` file in `views/` carry an array of view definitions instead of one. The single-view shape keeps parsing as-is; users with existing views see no change.

**Why it matters:** P4 (board), P5 (table), and P6 (timeline) all need to read the view's `display:` to decide what to render. The board needs `group_by:` to know which property to column-by. The table needs `columns:` to know which fields to show. Without these on the data model, every Track B UI phase has to invent its own ad-hoc binding to the YAML, and the formats will drift. P3 lands the schema once so the three view-mode phases share it.

The `file.X` field namespace is the other half of the same investment: the board needs to drag-filter "show only tasks I'm in this folder" type queries, and the timeline needs `file.mtime`. Building these into the resolver once means every filter we already have (`equals`, `before`, `contains`, `is_empty`) works uniformly across frontmatter, filesystem, and (eventually) computed fields.

**What you'll see when it's done:** nothing visible. P3 is backend-only — existing saved views render exactly the same. The proof is in tests: a multi-view `.yml` round-trips losslessly; a filter against `file.mtime` works; a view with `display: board, group_by: status` parses without error (the UI renderer is P4 work).

**Roughly how long:** 2 focused days.

---

## Context (technical)

The current view engine lives at [src-tauri/src/vault/views.rs](../../src-tauri/src/vault/views.rs) (482 lines), [view_migration.rs](../../src-tauri/src/vault/view_migration.rs), [view_date_filters.rs](../../src-tauri/src/vault/view_date_filters.rs), [view_relationships.rs](../../src-tauri/src/vault/view_relationships.rs), and [view_value_conversions.rs](../../src-tauri/src/vault/view_value_conversions.rs).

`ViewDefinition` today has: `name`, `icon`, `color`, `order`, `sort`, `list_properties_display`, `filters`. No `display`, no `group_by`, no `columns`.

Field resolution is in `resolve_condition_field` (views.rs:340). Known fields (`type`/`isA`, `status`, `title`, `body`) hardcoded; everything else falls through to `resolve_dynamic_condition_field` which looks at `entry.properties` then `entry.relationships`. There's no namespace — `mtime` would currently not resolve at all.

`view_date_filters.rs` already parses `today`, `yesterday`, `tomorrow`, `N days/weeks/months/years ago`, `in N days/...`. The strategic-plan acceptance test "file.mtime > now() - '1 week'" maps cleanly to the existing English-language syntax (`"1 week ago"`) — we do not need a new function-call parser. The `now()` / `today()` "functions" are already there as the bare strings `"today"`, `"1 week ago"`, etc.

`view_migration.rs` handles the legacy single-file-per-view format that predates the current single-view-per-file format. We extend it to recognize the new multi-view top-level shape.

## Decisions specific to P3

1. **Three new optional fields on `ViewDefinition`: `display`, `group_by`, `columns`.** All `Option<...>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` so existing view files stay byte-stable on round-trip. No new enum variants get added if a view doesn't opt in. Default for `display` (when absent) is treated as `list` at the consumer (frontend) — we do not synthesize the field on disk.
2. **`display: list | table | board | timeline | cards` is a closed enum.** Rust enum with `#[serde(rename_all = "lowercase")]`. Unknown values fail to parse. This is intentional — typos in `display:` should surface as YAML errors at view-load time, not silently fall back.
3. **`group_by` is `{ property: String, direction: Option<asc|desc> }`.** Direction is optional with no default; the view consumer (P4 board, P5 table) decides. Property is a fully-qualified field name (`note.status`, `file.folder`) and resolves through the same namespace resolver as filter fields.
4. **`columns` is `Vec<String>`.** Plain list of fully-qualified field names. Order matters. Empty array on disk serializes as omitted (skip_serializing_if).
5. **Field-namespace resolver is a new free function: `resolve_field(field: &str, entry: &VaultEntry) -> ConditionField<'_>`.** Replaces the current `resolve_condition_field` + `resolve_dynamic_condition_field` pair. Splits the input on the first `.`:
   - prefix `note` → look in `properties` then `relationships` then fail with `Scalar(None)`
   - prefix `file` → match against the locked list (`name`, `path`, `folder`, `ext`, `size`, `ctime`, `mtime`, `tags`)
   - prefix `formula` → return `Scalar(None)` and log a debug-level "formula.X is a v2 feature" warning once per session
   - no prefix → treat as `note.<field>`
   - explicit prefix `note` with a known structural alias (`type`, `isA`, `status`, `title`, `body`) → keep the current hardcoded resolution (back-compat with existing views that use these bare names without realizing they're structural rather than frontmatter properties)
6. **`file.tags` is the union of `belongsTo` + `relatedTo` wikilink targets.** Tolaria does not have a true tag system today — wikilinks in those fields are the closest analogue. P3 returns the union; if/when an explicit tag system lands later, this resolver swaps out without touching callers.
7. **`file.ctime` and `file.mtime` use the existing `created_at` / `modified_at` epoch-millis fields on `VaultEntry`.** Formatted as `YYYY-MM-DDTHH:MM:SS` so the existing date filter parser eats them.
8. **`file.size` is the existing `file_size: u64`.** Compared as integers via the same `Equals` / `Before` / `After` ops the date path uses — we coerce strings to numbers on both sides of the comparison.
9. **Multi-view file format:** an optional top-level `views:` key. When present, the file is a list of `ViewDefinition` bodies (each with its own `name`, `filters`, etc.). When absent, the file is a single `ViewDefinition` as today. Round-trip preserves the shape the file was loaded in.
10. **No new Tauri command, no new frontend code.** P3 is purely the parser + resolver. P4+ wire the new fields into the UI.
11. **All new files must reach CodeScene 10.0.** Existing files (views.rs at 482 lines) must not regress.

## Implementation breakdown

### Step 1 — Add `display`, `group_by`, `columns` to `ViewDefinition` (~1.5 hours)

In [views.rs](../../src-tauri/src/vault/views.rs):

```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ViewDisplay {
    List,
    Table,
    Board,
    Timeline,
    Cards,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection { Asc, Desc }

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GroupBy {
    pub property: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<SortDirection>,
}

// ViewDefinition gains:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub display: Option<ViewDisplay>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub group_by: Option<GroupBy>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub columns: Vec<String>,
```

Tests: existing view files (no new fields set) round-trip identically; a view with all three set round-trips; an unknown `display:` value fails to parse with a clear error.

### Step 2 — Field namespace resolver (~3 hours)

New free function `resolve_field` replacing `resolve_condition_field` + `resolve_dynamic_condition_field`. Lives in [views.rs](../../src-tauri/src/vault/views.rs) (or split into `view_fields.rs` if it gets long — decide at write time based on CodeScene score).

```rust
fn resolve_field<'a>(field: &str, entry: &'a VaultEntry) -> ConditionField<'a> {
    let (namespace, name) = field.split_once('.').unwrap_or(("note", field));
    match namespace {
        "note" => resolve_note_field(name, entry),
        "file" => resolve_file_field(name, entry),
        "formula" => formula_field_placeholder(name),
        _ => ConditionField::Scalar(None),
    }
}
```

`resolve_file_field` covers the eight locked names; `resolve_note_field` covers the four hardcoded structural aliases plus the existing properties/relationships fallback.

Tests:
- `note.priority` resolves the same as bare `priority`
- `file.folder` returns the parent folder of the entry's path
- `file.name` returns the title (or filename stem when no title)
- `file.path` returns the full path
- `file.ext` returns `md` for markdown notes
- `file.size` returns the integer byte size as a string
- `file.ctime` / `file.mtime` produce ISO 8601 strings that the existing date filter parser can compare
- `file.tags` returns belongsTo + relatedTo wikilink targets
- `formula.anything` resolves to empty + logs once
- Bare `priority` still resolves (back-compat)
- Bare `type` still resolves to the structural alias (back-compat)

### Step 3 — End-to-end filter cases (~1 hour)

Wire the new resolver into `evaluate_condition`. Add focused tests that exercise the new namespaces through real filter conditions:

- `field: file.mtime, op: after, value: "1 week ago"` — finds entries modified in the last week
- `field: file.folder, op: equals, value: "Projects/Active"` — finds entries in a specific folder
- `field: file.tags, op: contains, value: "[[urgent]]"` — finds entries whose belongsTo or relatedTo references include the target

### Step 4 — Multi-view file format (~2 hours)

Edit `read_view_file` and `save_view` in [views.rs](../../src-tauri/src/vault/views.rs):

```rust
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawViewFile {
    Multi { views: Vec<ViewDefinition> },
    Single(ViewDefinition),
}
```

`scan_views` flattens the multi-view variant into one `ViewFile` per definition, with synthetic filenames `{base}.yml#{index}` so the frontend can distinguish them. On save, group by base filename and serialize back into whichever shape the file was loaded in — track the shape on `ViewFile`.

[view_migration.rs](../../src-tauri/src/vault/view_migration.rs) doesn't need a new migration — the new multi-view shape is a superset of the existing single-view shape and round-trips cleanly. Document this explicitly in a comment so future me doesn't accidentally try to "fix" it.

Tests:
- Round-trip a single-view file (existing test, plus add an explicit byte-stability assertion)
- Round-trip a multi-view file → load → save → re-load → assert structural equality and key order
- Mixed views directory (one single-view file + one multi-view file) → scan returns 1 + N entries with stable filenames

### Step 5 — CodeScene + Codacy + commit per step (~1 hour)

Per [AGENTS.md §2](../../AGENTS.md#2-development-process):

- After each step's commit, run a CodeScene file-level check on every touched/new file. New files must hit 10.0. Touched files must not regress.
- Codacy MCP/CLI scan on every touched file. No new Critical/High findings.
- Frontend coverage gate: no FE files changed, so frontend coverage stays where it was. Rust coverage stays ≥85% on touched files (add tests as needed to maintain).

### Step 6 — Docs (~30 min)

- [docs/ARCHITECTURE.md](../ARCHITECTURE.md): one paragraph in "View Engine" (or under "Saved Views" if no view-engine section exists yet) describing the namespace resolver and multi-view file format.
- [docs/ABSTRACTIONS.md](../ABSTRACTIONS.md): add `ViewDisplay`, `GroupBy`, and `resolve_field` to the Rust abstractions table.

### Step 7 — Push (~1 hour)

Standard pre-push gate: `tsc` + `vite build` (no FE changes; should be a no-op build), frontend coverage (unchanged), `cargo clippy + fmt`, `cargo llvm-cov ≥85%` on touched files, Playwright smoke (unchanged), CodeScene (still skipped on this Mac without PAT — CI will enforce).

## Acceptance criteria

1. Loading an existing view file (no new fields set) parses successfully and round-trips byte-for-byte through scan_views → save_view.
2. Loading a view with `display: board, group_by: { property: status }, columns: [name, status, due]` parses successfully and serializes back losslessly.
3. Loading a view with `display: invalidvalue` fails to parse with a YAML error that names the field.
4. A filter `{ field: file.mtime, op: after, value: "1 week ago" }` returns only entries with `modifiedAt` in the last 7 days.
5. A filter `{ field: file.folder, op: equals, value: "Projects/Active" }` returns only entries whose path's parent folder matches.
6. A filter `{ field: file.tags, op: contains, value: "[[urgent]]" }` returns entries whose belongsTo or relatedTo includes that wikilink target.
7. A filter `{ field: formula.anything, op: equals, value: x }` returns no matches (resolves to empty) and emits a one-off debug log.
8. A multi-view file with two view bodies under `views:` produces two `ViewFile` entries; saving them back preserves the multi-view shape.
9. CodeScene Hotspot + Average stay ≥ `.codescene-thresholds`. No new Critical/High Codacy findings.

## Out of scope for P3

- All Track B UI (board P4, table P5, timeline P6, cards in the same family as cards-in-list).
- Function-call filter syntax (`now()`, `today()`, `file.hasTag()` as predicates). The strategic plan listed these as v1 helper functions — P3 ships the equivalent declarative form (`field: file.tags, op: contains, value: ...`) which is simpler and works with the existing operator set. If users explicitly ask for the function-call form later, P3.5 can add a parser layer on top.
- `formula.X` evaluation. The namespace is reserved and any reference resolves to empty.
- Migrating existing views to use namespaced fields. Bare names keep working; no rewrite needed.
- A new Tauri command. P3 only changes the parser and the resolver, both internal to the Rust crate.
