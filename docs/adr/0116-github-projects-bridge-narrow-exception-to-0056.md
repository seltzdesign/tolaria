---
type: ADR
id: "0116"
title: "GitHub Projects v2 bridge: narrow exception to ADR 0056"
status: active
date: 2026-05-12
supersedes: "0056"
---

## Context

[ADR 0056](0056-system-git-cli-auth-no-provider-oauth.md) removed all GitHub-specific authentication and repository APIs from Tolaria. The product stayed on a single auth path — the user's existing system `git` configuration — and every git operation (clone, commit, push, pull, status) shells out to the system `git` executable. That decision held cleanly because every Tolaria feature up to this point only needed git transport.

The Tasks feature (see [ADR 0115](0115-tasks-and-projects-as-typed-notes.md) and parent plan `docs/plans/001-task-in-tolaria.md`) introduces a bidirectional bridge to GitHub Projects v2. Projects v2 is GraphQL-only, behind `api.github.com`, and requires a token bound to a user account. There is no git-transport path that reaches the Projects API; the system-`git` route cannot be reused. The bridge therefore needs an authenticated client, which means re-introducing a GitHub-specific auth path that ADR 0056 retired.

The challenge is to do this without re-opening the door ADR 0056 closed. The bridge must be narrow (Projects-only, never used for git transport), opt-in (a user who never binds a project has no functional difference from a no-PAT install), and never persist credentials in any user-readable file.

## Decision

**Tolaria re-introduces GitHub API authentication, scoped exclusively to the GitHub Projects v2 bridge. ADR 0056's principle remains in force everywhere else: all git transport continues through system `git` configuration, with no provider-specific code in the git path.**

Specifically:

1. The bridge module at `src-tauri/src/github/projects/` is the only code path in Tolaria that calls `api.github.com`. Git commit, push, pull, clone, and status code continues to shell out to system `git` with no token injection.
2. The PAT is stored in the OS keychain via the `keyring` crate under service name `com.tolaria.app.github_pat`. It does NOT live in `settings.json`, `~/.config/tolaria/`, any vault file, any log, or any telemetry payload.
3. Tolaria accepts both PAT formats and detects which is which by prefix:
   - **Fine-grained PAT** (`github_pat_*`, recommended): user must grant `Projects: read/write` plus `Issues: read/write` on the repositories they plan to link issues from (only required when a project enables `link_to_issues`).
   - **Classic PAT** (`ghp_*`): user grants the `repo` and `project` scopes.
   - The Settings UI surfaces the required scope list for each prefix when the user pastes a token.
4. The bridge is opt-in per project. A user who never sets `sync_enabled: true` on any project note has no functional difference from a no-PAT install — the bridge module loads but never makes a network request.
5. PAT presence is a precondition for binding a project. The Bind Project flow ([ADR 0115 §3](0115-tasks-and-projects-as-typed-notes.md)) refuses to open until a `viewer { login }` query against the current PAT returns 200. The Settings UI exposes a "Test connection" button that runs the same query on demand.
6. PAT rotation is the user's responsibility. When any bridge request returns 401, the bridge pauses sync for all bound projects and surfaces a non-blocking banner prompting the user to re-enter the PAT. Existing bindings stay valid across rotations because they store project node IDs, not credentials.
7. Telemetry never includes the PAT or any field derived from it. The token *type* (fine-grained vs classic) IS sent as an anonymous event property on `github_sync_started` so we can see adoption mix; the token itself is never read into a telemetry context.

## Alternatives considered

- **Pure system-git auth + manage Projects via `gh project` CLI** (status quo, ADR 0056 untouched): defeats the purpose of in-app task UI. The user explicitly wants tasks visible and editable inside Tolaria. Rejected.
- **Store PAT in `settings.json`**: simplest implementation, but secrets in a plain-text settings file are unacceptable on a shared device, in a synced settings backup, or in a crash report. Rejected.
- **GitHub Device Flow OAuth** (the original [ADR 0019](0019-github-device-flow-oauth.md) approach): smoother first-time UX but a large auth stack — token refresh, scope reconciliation, OAuth app registration — for a single feature. PAT entry is a one-time cost the user is already familiar with from `gh`. Rejected.
- **Fine-grained PAT only**: more secure and the GitHub-recommended path, but a meaningful fraction of users have classic PATs already in place from existing tooling. Forcing token migration adds setup friction without proportional security benefit on a personal-use desktop app. Rejected in favor of supporting both.
- **System-wide keychain entry shared with `gh` CLI**: would inherit credentials the user already configured. Rejected because `gh`'s scope set is broader than the bridge needs (it grants `repo` + `read:org` + others), so reusing it would over-grant access compared to a Tolaria-owned PAT scoped to the minimum required.

## Consequences

- New crate dependency: `keyring = "3"`. Cross-platform (macOS Keychain, Windows Credential Manager, GNOME Keyring / KWallet on Linux).
- New module: `src-tauri/src/github/projects/` containing `auth.rs`, `client.rs`, `sync.rs`, `scheduler.rs`, `snapshot.rs`, `rate_limit.rs`. The module is the only place `reqwest::Client` is constructed for `api.github.com`.
- The bridge inherits no git credentials. It will not reuse a system git PAT even if one is available, because git PATs may have scopes outside what the bridge needs (over-grant) or may lack the `project` scope entirely (under-grant). The Tolaria PAT is independent.
- Loss of the OS keychain entry — uninstall + reinstall, OS migration, user manually clearing credentials — does not corrupt bound project state. The user re-enters the PAT in Settings; project bindings remain valid because they reference project node IDs, not credentials.
- A user can audit which Tolaria reads/writes happen on GitHub by filtering `api.github.com` requests in the network panel; nothing else in the app touches that host.
- Re-evaluate if: Tolaria adds support for another remote provider (GitLab, Gitea) — at that point we likely promote the keychain-storage pattern into a generic remote-credential abstraction rather than a Projects-only special case.
