---
type: ADR
id: "0119"
title: "Three-way diff with last-write-wins for GitHub Projects sync"
status: active
date: 2026-05-12
---

## Context

The GitHub Projects bridge ([ADR 0116](0116-github-projects-bridge-narrow-exception-to-0056.md)) is bidirectional: local edits to a `.md` task push to GitHub Projects v2, and remote edits made on github.com pull back into the local frontmatter. Without an explicit conflict policy, simultaneous edits on both sides silently overwrite each other — the order in which the next sync cycle runs determines which version survives, and the lost edit leaves no trace.

A conflict policy has to handle three concerns:

1. **Detection.** How do we know an edit on one side happened without overwriting it from the other? A two-way diff (current local vs. current remote) cannot distinguish "you changed nothing and I changed it" from "we both changed it" — both look like a difference. A three-way diff using a stored snapshot of the last successful sync as the common ancestor avoids that.
2. **Resolution.** When both sides have legitimately diverged, something has to win. Manual resolution UI gives the user control but is significant UX surface. Last-write-wins by timestamp is simple, deterministic, and adequate for a personal-use bridge where the user controls both sides.
3. **Recoverability.** LWW must never silently delete the losing edit. The losing version needs to land somewhere the user can retrieve it from.

The bridge is opt-in per project ([ADR 0116 §4](0116-github-projects-bridge-narrow-exception-to-0056.md)). For a personal-fork product where the user is also the only end-user, manual conflict resolution would be overkill for the first release; LWW with recoverable copies meets the bar.

## Decision

**Sync conflict detection uses a three-way diff against a per-project snapshot stored in the cache directory. On conflict, last-write-wins by timestamp, with the losing version preserved as a recoverable conflict copy. A manual resolution UI is explicitly deferred to v2.**

Specifically:

1. Each bound project maintains a snapshot at `<cache-dir>/github-sync/<project_node_id>.json`. The file lives in the cache directory per [ADR 0024](0024-cache-outside-vault.md), not in the vault — sync state is not user data. The snapshot stores, for every item the bridge has seen: the GitHub item node ID, the local file path, the full set of remote field values from the last successful sync, the `synced_at` timestamp of that sync, and a `github_remote_snapshot_hash` covering the remote field set.
2. Per-item reconciliation, computed once per sync cycle:
   - `local_changed = (local file mtime > snapshot.synced_at) OR (current frontmatter content hash ≠ snapshot.local_hash)`
   - `remote_changed = (remote updatedAt > snapshot.synced_at)`
   - **Both false:** no-op.
   - **Local only:** push local → remote, update snapshot.
   - **Remote only:** pull remote → local, update snapshot.
   - **Both:** conflict; apply last-write-wins per decision 3.

   The frontmatter content hash **excludes local-only fields** — fields the user maintains that have no corresponding remote representation. v1 local-only fields: `blocked_by` (see [ADR 0115 §4](0115-tasks-and-projects-as-typed-notes.md)). Editing only a local-only field does not trigger a push and does not mark the task as `local_changed`. The hash also excludes the `github_*` bridge-managed fields ([ADR 0115 §2](0115-tasks-and-projects-as-typed-notes.md)), since those are sync-internal and not user edits.
3. LWW comparison: local file `mtime` (filesystem authoritative — the bridge does NOT trust the frontmatter `github_last_synced` field for comparison because it can be hand-edited or stale) vs. remote `updatedAt` from the GraphQL response. Whichever is newer wins.
4. Tie-breaker for genuinely equal timestamps: **remote wins**. Ties require same-second edits on both sides during a disconnected window, which is rare. When in doubt, favoring github.com matches the "GitHub is the project tracker of record" mental model and is the less-surprising default. Choosing remote in ties also avoids the worst case where a stale local clock makes local always-newer.
5. The losing version is preserved as a conflict copy at `<cache-dir>/github-sync/conflicts/<task_node_id>.<unix-timestamp>.md`. The file contains the loser's full frontmatter and body — it is a valid markdown note that the user can copy back into the vault if they want. Conflict copies are kept indefinitely; the user clears them manually when no longer needed.
6. Every conflict resolution writes an audit entry to `.tolaria/sync-log.jsonl` (in the vault, gitignored per existing conventions). Entry schema: `{timestamp, project_node_id, task_node_id, task_title, winner: "local" | "remote", local_mtime, remote_updated_at, local_hash, remote_hash, conflict_copy_path}`. The log is append-only in v1.
7. Body content (markdown body, not just frontmatter) participates in conflict detection and resolution. The hash in the snapshot covers both frontmatter and body. The winning side's body is written to the local file; the losing side's body is preserved in the conflict copy.
8. The local task's `github_sync_status` frontmatter field ([ADR 0115 §2](0115-tasks-and-projects-as-typed-notes.md)) is set to `conflicted` for the duration of a conflict — that is, from the moment a conflict copy is created until the next successful reconcile cycle where the task is no longer in conflict (typically the cycle after the user reviews the conflict copy and either accepts the winner or merges manually). On successful reconcile, `github_sync_status` returns to `synced`.
9. Initial-sync-after-no-snapshot: if the snapshot file is missing — first-ever sync of a project, or cache wipe — every task would otherwise look like "both changed" under the rules in decision 2 and produce a flood of false conflicts. The bridge handles this with a one-pass bootstrap: treat the remote state as the snapshot baseline (no `local_changed`, no `remote_changed` for items already at parity), write the snapshot, and continue. The user is informed via a one-time banner: `"Project '<title>' synced from GitHub — local edits made before this sync are preserved on disk and will sync on next save."`

## Alternatives considered

- **Three-way diff + LWW + recoverable conflict copies** (chosen): deterministic, simple, never silently loses data. Defers the UX surface of manual resolution to a v2 ADR when usage patterns reveal whether it is actually needed.
- **Manual resolution UI per conflict**: the obviously-correct approach for a team product, but a sizable UX surface — conflict queue, side-by-side diff, partial accept — for v1 of a personal-use bridge. Deferred to v2; the conflict copies + audit log in v1 preserve the data such that a v2 UI can retroactively offer resolution.
- **CRDT-based merging** (e.g., Yjs, Automerge for frontmatter): would auto-merge non-conflicting field changes without user intervention. Rejected because GitHub Project fields are not CRDT-friendly (single-select / date / number fields, no last-writer encoding), and forcing CRDT semantics on github.com state would require maintaining shadow data the bridge can never publish.
- **"Local always wins" or "Remote always wins"**: trivially simple and never asks the user, but silently destroys whichever side loses every time. Rejected — even for personal use, this loses too much data over time.
- **Snapshot stored inside the vault** (e.g., `.tolaria/github-sync/`): would survive cache wipes via git, but pollutes the vault with bridge implementation state and creates merge-conflict risk in the snapshot itself when the vault is git-synced across machines. Rejected per [ADR 0024](0024-cache-outside-vault.md); the bootstrap path in decision 9 handles cache loss adequately.
- **Compare local `github_last_synced` instead of file mtime**: the frontmatter timestamp is user-visible and could be hand-edited or copy-pasted; file mtime is filesystem-authoritative. Rejected.

## Consequences

- Loss of the cache directory (uninstall, OS migration, manual delete) is recoverable without conflict floods thanks to the bootstrap path in decision 9. The cost is that local edits made between the last sync and the cache loss DO get re-evaluated against the remote state on the next sync — local-only edits push correctly because their hashes differ from the bootstrap baseline; concurrent edits produce real conflicts that LWW resolves.
- The `.tolaria/sync-log.jsonl` file grows monotonically. v1 accepts this; v2 may add log rotation or pruning.
- Conflict copies accumulate in `<cache-dir>/github-sync/conflicts/` until the user clears them. The directory is not part of the vault and is never synced, so the disk cost is local. A future ADR may add an automated retention policy (e.g., delete after 90 days).
- The v1 user-facing message when a conflict resolves: `"Conflict on '<task title>': remote version won — your local version saved to <path>"` (or vice versa). The message is non-modal; the user can keep working.
- Body content participating in conflict resolution means a user who edits a task body locally and on github.com concurrently will see the losing body in the conflict copy. This is the same behavior as frontmatter — there is no per-field merge.
- Re-evaluate if: real-world usage shows users frequently want to merge field-by-field (suggesting a manual resolution UI is overdue), or if cache-loss conflict floods turn out to be common (suggesting the snapshot belongs in the vault after all).
