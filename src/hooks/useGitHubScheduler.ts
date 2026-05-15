/**
 * Hook that starts and stops the Rust-side GitHub Projects sync
 * scheduler in lockstep with the active vault.
 *
 * The scheduler scans the vault for bound projects with
 * `sync_enabled: true` and runs `pull + push` for each at its
 * configured interval. It needs to be re-kicked whenever the vault
 * path changes (different vault → different bindings) and stopped on
 * unmount. Lifecycle events (`github_sync_started`,
 * `github_sync_finished`, `github_sync_error`) are exposed so
 * components like the project header can show spinners or surface
 * conflicts without polling.
 *
 * The current authoritative source of recent runs is the renderer's
 * existing vault watcher: when a sync writes a task file on disk, the
 * watcher detects the FS event and refreshes the open editor tab —
 * no special wiring is needed here for the autosave/sync race fix.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface SyncFinishedPayload {
  project_node_id: string
  trigger: 'manual' | 'scheduled'
  result: {
    created: number
    updated: number
    deleted: number
    unchanged: number
    items_seen: number
    items_skipped: number
    conflicts: number
    pushed_creates: number
    pushed_field_updates: number
    touched_paths: string[]
    warnings: string[]
    errors: string[]
  }
}

export interface SyncErrorPayload {
  project_node_id?: string | null
  trigger: 'manual' | 'scheduled'
  message: string
}

interface UseGitHubSchedulerOptions {
  vaultPath: string | null | undefined
  onSyncFinished?: (payload: SyncFinishedPayload) => void
  onSyncError?: (payload: SyncErrorPayload) => void
}

interface UseGitHubSchedulerResult {
  refresh: () => void
}

/**
 * Boot the background scheduler for `vaultPath` and tear it down on
 * unmount or vault switch. `refresh()` re-scans bindings — call it
 * after a Bind / Unbind so newly bound projects start ticking
 * immediately rather than at the next app start.
 */
export function useGitHubScheduler({
  vaultPath,
  onSyncFinished,
  onSyncError,
}: UseGitHubSchedulerOptions): UseGitHubSchedulerResult {
  const finishedRef = useRef(onSyncFinished)
  const errorRef = useRef(onSyncError)
  useEffect(() => {
    finishedRef.current = onSyncFinished
  }, [onSyncFinished])
  useEffect(() => {
    errorRef.current = onSyncError
  }, [onSyncError])

  const [version, setVersion] = useState(0)
  const refresh = useCallback(() => setVersion((n) => n + 1), [])

  useEffect(() => {
    if (!vaultPath) return
    let cancelled = false
    let cleanups: UnlistenFn[] = []

    // `invoke` returns undefined under the mock-tauri harness for any
    // command without a registered handler, so we have to wrap the call
    // in `Promise.resolve` before chaining `.catch` — otherwise the test
    // run hits "Cannot read properties of undefined (reading 'catch')".
    Promise.resolve(invoke('github_scheduler_start', { vaultPath })).catch(
      (err) => {
        console.warn('github_scheduler_start failed:', err)
      },
    )

    Promise.resolve(
      listen<SyncFinishedPayload>('github_sync_finished', (event) => {
        if (cancelled) return
        finishedRef.current?.(event.payload)
      }),
    )
      .then((unlisten) => {
        if (!unlisten) return
        if (cancelled) unlisten()
        else cleanups.push(unlisten)
      })
      .catch(() => {})

    Promise.resolve(
      listen<SyncErrorPayload>('github_sync_error', (event) => {
        if (cancelled) return
        errorRef.current?.(event.payload)
      }),
    )
      .then((unlisten) => {
        if (!unlisten) return
        if (cancelled) unlisten()
        else cleanups.push(unlisten)
      })
      .catch(() => {})

    return () => {
      cancelled = true
      for (const fn of cleanups) fn()
      cleanups = []
      Promise.resolve(invoke('github_scheduler_stop')).catch(() => {})
    }
  }, [vaultPath, version])

  return { refresh }
}
