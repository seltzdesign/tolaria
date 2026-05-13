import type { VaultEntry } from '../../types'

/**
 * Frontend mirror of the Rust [`ProjectView`](../../../src-tauri/src/vault/task.rs).
 * Reads project metadata on demand from `VaultEntry.properties`. The entry
 * remains the source of truth.
 */

const DEFAULT_SYNC_INTERVAL_MINUTES = 5

function propertyString(entry: VaultEntry, key: string): string | null {
  const value = entry.properties[key]
  return typeof value === 'string' ? value : null
}

function propertyBoolean(entry: VaultEntry, key: string): boolean {
  const value = entry.properties[key]
  return typeof value === 'boolean' ? value : false
}

function propertyU32(entry: VaultEntry, key: string, fallback: number): number {
  const value = entry.properties[key]
  if (typeof value === 'number' && Number.isFinite(value) && value >= 0) {
    return Math.floor(value)
  }
  return fallback
}

function propertyStrings(entry: VaultEntry, key: string): string[] {
  const value = entry.properties[key]
  if (Array.isArray(value)) return value.filter((item): item is string => typeof item === 'string')
  if (typeof value === 'string') return [value]
  return []
}

export class ProjectView {
  constructor(private readonly entry: VaultEntry) {}

  get taskFolder(): string | null {
    return propertyString(this.entry, 'task_folder')
  }

  get statuses(): string[] {
    return propertyStrings(this.entry, 'statuses')
  }

  /**
   * Per ADR 0115 §3: defaults to `['Done']` if statuses contains "Done"
   * (case-insensitive), otherwise the last status, otherwise empty.
   */
  get terminalStatuses(): string[] {
    const explicit = propertyStrings(this.entry, 'terminal_statuses')
    if (explicit.length > 0) return explicit
    const statuses = this.statuses
    if (statuses.some((s) => s.toLowerCase() === 'done')) return ['Done']
    const last = statuses[statuses.length - 1]
    return last ? [last] : []
  }

  get defaultView(): string | null {
    return propertyString(this.entry, 'default_view')
  }

  get syncEnabled(): boolean {
    return propertyBoolean(this.entry, 'sync_enabled')
  }

  get syncIntervalMinutes(): number {
    return propertyU32(this.entry, 'sync_interval_minutes', DEFAULT_SYNC_INTERVAL_MINUTES)
  }

  get linkToIssues(): boolean {
    return propertyBoolean(this.entry, 'link_to_issues')
  }

  get githubProjectUrl(): string | null {
    return propertyString(this.entry, 'github_project_url')
  }

  get githubProjectNodeId(): string | null {
    return propertyString(this.entry, 'github_project_node_id')
  }

  get githubIssueRepo(): string | null {
    return propertyString(this.entry, 'github_issue_repo')
  }

  get statusField(): string | null {
    return propertyString(this.entry, 'status_field')
  }
}

export function isProjectEntry(entry: VaultEntry): boolean {
  return entry.isA === 'project'
}

export function asProject(entry: VaultEntry): ProjectView | null {
  return isProjectEntry(entry) ? new ProjectView(entry) : null
}
