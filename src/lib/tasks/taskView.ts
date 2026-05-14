import type { VaultEntry } from '../../types'
import { parseDateOrDateTime, type DateOrDateTime } from './dateOrDateTime'

/**
 * Frontend mirror of the Rust [`TaskView`](../../../src-tauri/src/vault/task.rs).
 * Reads typed fields on demand from `VaultEntry.properties` (scalars, labels)
 * and `VaultEntry.relationships` (wikilink fields). The entry remains the source
 * of truth; this is a read-only wrapper.
 */

function wikilinkTarget(raw: string): string | null {
  const inner = raw.startsWith('[[') && raw.endsWith(']]') ? raw.slice(2, -2) : null
  if (inner === null) return null
  const target = inner.includes('|') ? inner.slice(0, inner.indexOf('|')) : inner
  const trimmed = target.trim()
  return trimmed ? trimmed : null
}

function wikilinkTargets(raw: readonly string[] | undefined): string[] {
  if (!raw) return []
  const targets: string[] = []
  for (const value of raw) {
    const target = wikilinkTarget(value)
    if (target !== null) targets.push(target)
  }
  return targets
}

function propertyString(entry: VaultEntry, key: string): string | null {
  const value = entry.properties[key]
  return typeof value === 'string' ? value : null
}

function propertyNumber(entry: VaultEntry, key: string): number | null {
  const value = entry.properties[key]
  if (typeof value === 'number' && Number.isFinite(value)) return value
  return null
}

function propertyBoolean(entry: VaultEntry, key: string): boolean | null {
  const value = entry.properties[key]
  return typeof value === 'boolean' ? value : null
}

function propertyStrings(entry: VaultEntry, key: string): string[] {
  const value = entry.properties[key]
  if (Array.isArray(value)) return value.filter((item): item is string => typeof item === 'string')
  if (typeof value === 'string') return [value]
  return []
}

function relationshipTargets(entry: VaultEntry, key: string): string[] {
  return wikilinkTargets(entry.relationships[key])
}

function parseDateProperty(entry: VaultEntry, key: string): DateOrDateTime | null {
  const raw = propertyString(entry, key)
  return raw ? parseDateOrDateTime(raw) : null
}

export class TaskView {
  constructor(private readonly entry: VaultEntry) {}

  get status(): string | null {
    return this.entry.status
  }

  get priority(): string | null {
    return propertyString(this.entry, 'priority')
  }

  get due(): DateOrDateTime | null {
    return parseDateProperty(this.entry, 'due')
  }

  get start(): DateOrDateTime | null {
    return parseDateProperty(this.entry, 'start')
  }

  get completed(): DateOrDateTime | null {
    return parseDateProperty(this.entry, 'completed')
  }

  get estimate(): number | null {
    return propertyNumber(this.entry, 'estimate')
  }

  get completion(): number | null {
    const raw = propertyNumber(this.entry, 'completion')
    if (raw === null) return null
    if (raw < 0) return 0
    if (raw > 100) return 100
    return Math.round(raw)
  }

  get labels(): string[] {
    return propertyStrings(this.entry, 'labels')
  }

  get project(): string | null {
    return relationshipTargets(this.entry, 'project')[0] ?? null
  }

  get assignees(): string[] {
    return relationshipTargets(this.entry, 'assignee')
  }

  get blockedBy(): string[] {
    return relationshipTargets(this.entry, 'blocked_by')
  }

  get githubSyncStatus(): string | null {
    return propertyString(this.entry, 'github_sync_status')
  }

  get githubItemNodeId(): string | null {
    return propertyString(this.entry, 'github_item_node_id')
  }

  get githubProjectNodeId(): string | null {
    return propertyString(this.entry, 'github_project_node_id')
  }

  get githubIssueUrl(): string | null {
    return propertyString(this.entry, 'github_issue_url')
  }

  get githubLastSynced(): string | null {
    return propertyString(this.entry, 'github_last_synced')
  }

  get githubRemoteSnapshotHash(): string | null {
    return propertyString(this.entry, 'github_remote_snapshot_hash')
  }
}

export function isTaskEntry(entry: VaultEntry): boolean {
  return entry.isA === 'task'
}

export function asTask(entry: VaultEntry): TaskView | null {
  return isTaskEntry(entry) ? new TaskView(entry) : null
}

export const __internal = { wikilinkTarget, propertyBoolean }
