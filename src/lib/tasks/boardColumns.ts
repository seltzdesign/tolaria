import type { VaultEntry, ViewGroupBy } from '../../types'
import { asProject } from './projectView'
import { asTask } from './taskView'

export const BOARD_COLUMN_ID_PREFIX = 'column:'

export interface BoardDropResult {
  path: string
  field: string
  from: string | null
  to: string | null
}

export interface BoardColumn {
  /** Value stored on `entry.properties[field]`. Empty string for the unset column. */
  key: string
  /** Display label. */
  label: string
  /** True when this column buckets entries with no value for the group-by field. */
  isUnset: boolean
  entries: VaultEntry[]
}

/**
 * Strip a `note.` namespace prefix from a group-by field name. We do not allow
 * `file.X` group-by because those values are derived and not writable; the UI
 * should not surface a board for such views, but if it does, this still returns
 * the bare name and the value-read path will produce empty buckets.
 */
export function normalizeBoardField(property: string): string {
  if (property.startsWith('note.')) return property.slice('note.'.length)
  return property
}

function readFieldValue(entry: VaultEntry, field: string): string | null {
  if (field === 'status') return entry.status ?? null
  const value = entry.properties[field]
  if (typeof value === 'string') return value
  if (typeof value === 'number' || typeof value === 'boolean') return String(value)
  return null
}

function parseColumnId(id: unknown): string | null {
  if (typeof id !== 'string') return null
  return id.startsWith(BOARD_COLUMN_ID_PREFIX) ? id.slice(BOARD_COLUMN_ID_PREFIX.length) : null
}

export function planBoardDrop(args: {
  filteredEntries: VaultEntry[]
  columns: BoardColumn[]
  writeField: string
  activeId: unknown
  overId: unknown
}): BoardDropResult | null {
  const { filteredEntries, columns, writeField, activeId, overId } = args
  const path = typeof activeId === 'string' ? activeId : null
  const targetKey = parseColumnId(overId)
  if (path === null || targetKey === null) return null
  const entry = filteredEntries.find((candidate) => candidate.path === path)
  if (!entry) return null
  const target = columns.find((column) => column.key === targetKey)
  if (!target) return null
  const currentValue = readFieldValue(entry, writeField)
  const desiredValue = target.isUnset ? null : target.key
  if (currentValue === desiredValue) return null
  return { path, field: writeField, from: currentValue, to: desiredValue }
}

function bindingProjectStatuses(filtered: VaultEntry[], allEntries: VaultEntry[]): string[] | null {
  for (const entry of filtered) {
    const task = asTask(entry)
    if (!task?.project) continue
    const project = allEntries.find(
      (candidate) => candidate.isA === 'project' && projectMatchesTarget(candidate, task.project!),
    )
    if (!project) continue
    const projectView = asProject(project)
    if (!projectView) continue
    if (projectView.statuses.length > 0) return projectView.statuses
  }
  return null
}

function projectMatchesTarget(project: VaultEntry, target: string): boolean {
  if (project.title === target) return true
  if (project.aliases.includes(target)) return true
  return false
}

function distinctValues(filtered: VaultEntry[], field: string): { values: string[]; hasUnset: boolean } {
  const seen = new Set<string>()
  const values: string[] = []
  let hasUnset = false
  for (const entry of filtered) {
    const raw = readFieldValue(entry, field)
    if (raw === null || raw === '') {
      hasUnset = true
      continue
    }
    if (!seen.has(raw)) {
      seen.add(raw)
      values.push(raw)
    }
  }
  return { values, hasUnset }
}

function bucketEntries(filtered: VaultEntry[], field: string, columns: BoardColumn[]): void {
  const byKey = new Map<string, BoardColumn>(columns.map((column) => [column.key, column]))
  const unsetColumn = columns.find((column) => column.isUnset)
  for (const entry of filtered) {
    const raw = readFieldValue(entry, field)
    if (raw === null || raw === '') {
      unsetColumn?.entries.push(entry)
      continue
    }
    const column = byKey.get(raw)
    if (column) column.entries.push(entry)
    else unsetColumn?.entries.push(entry)
  }
}

export function deriveBoardColumns(
  filtered: VaultEntry[],
  allEntries: VaultEntry[],
  groupBy: ViewGroupBy,
  unsetLabel = '(unset)',
): BoardColumn[] {
  const field = normalizeBoardField(groupBy.property)

  let valueColumns: string[]
  let needsUnset: boolean
  const projectStatuses = field === 'status' ? bindingProjectStatuses(filtered, allEntries) : null
  if (projectStatuses) {
    valueColumns = projectStatuses
    needsUnset = filtered.some((entry) => {
      const raw = readFieldValue(entry, field)
      return raw === null || raw === '' || !projectStatuses.includes(raw)
    })
  } else {
    const distinct = distinctValues(filtered, field)
    valueColumns = distinct.values
    needsUnset = distinct.hasUnset
  }

  const columns: BoardColumn[] = valueColumns.map((value) => ({
    key: value,
    label: value,
    isUnset: false,
    entries: [],
  }))
  if (needsUnset) {
    columns.push({ key: '', label: unsetLabel, isUnset: true, entries: [] })
  }

  bucketEntries(filtered, field, columns)
  return columns
}
