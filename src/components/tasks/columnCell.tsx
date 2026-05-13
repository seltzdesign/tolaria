import type { ReactNode } from 'react'
import { Badge } from '../ui/badge'
import type { VaultEntry } from '../../types'
import type { AppLocale, TranslationKey } from '../../lib/i18n'
import { translate } from '../../lib/i18n'
import { asTask } from '../../lib/tasks/taskView'
import { parseDateOrDateTime, type DateOrDateTime } from '../../lib/tasks/dateOrDateTime'
import type { SortOption } from '../../utils/noteListHelpers'

const PRIORITY_VARIANTS: Record<string, 'destructive' | 'default' | 'secondary' | 'outline'> = {
  P0: 'destructive',
  P1: 'default',
  P2: 'secondary',
  P3: 'outline',
}

const KNOWN_COLUMN_LABELS: Record<string, TranslationKey> = {
  title: 'tasks.table.column.title',
  status: 'tasks.table.column.status',
  priority: 'tasks.table.column.priority',
  due: 'tasks.table.column.due',
  start: 'tasks.table.column.start',
  completed: 'tasks.table.column.completed',
  assignees: 'tasks.table.column.assignees',
  labels: 'tasks.table.column.labels',
  project: 'tasks.table.column.project',
  estimate: 'tasks.table.column.estimate',
  'file.name': 'tasks.table.column.fileName',
  'file.mtime': 'tasks.table.column.fileMtime',
  'file.ctime': 'tasks.table.column.fileCtime',
  'file.path': 'tasks.table.column.filePath',
  'file.folder': 'tasks.table.column.fileFolder',
  'file.ext': 'tasks.table.column.fileExt',
  'file.size': 'tasks.table.column.fileSize',
  'file.tags': 'tasks.table.column.fileTags',
}

export function getColumnLabel(name: string, locale: AppLocale): string {
  const stripped = stripNotePrefix(name)
  const key = KNOWN_COLUMN_LABELS[stripped]
  if (key) return translate(locale, key)
  return stripped
}

export function getColumnSortOption(name: string): SortOption | null {
  const stripped = stripNotePrefix(name)
  if (stripped === 'title') return 'title'
  if (stripped === 'status') return 'status'
  if (stripped === 'file.mtime') return 'modified'
  if (stripped === 'file.ctime') return 'created'
  if (stripped.startsWith('file.')) return null
  return `property:${stripped}`
}

function stripNotePrefix(name: string): string {
  return name.startsWith('note.') ? name.slice('note.'.length) : name
}

function formatDate(value: DateOrDateTime, locale: AppLocale): string {
  if (value.kind === 'date') {
    const [year, month, day] = value.date.split('-').map(Number)
    return new Date(year, month - 1, day).toLocaleDateString(locale, { month: 'short', day: 'numeric' })
  }
  return new Date(value.iso).toLocaleString(locale, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })
}

function dueVariant(value: DateOrDateTime): 'destructive' | 'default' | 'secondary' {
  const todayKey = new Date().toISOString().slice(0, 10)
  const dueKey = value.kind === 'date' ? value.date : value.iso.slice(0, 10)
  if (dueKey < todayKey) return 'destructive'
  if (dueKey === todayKey) return 'default'
  return 'secondary'
}

function renderTitle(entry: VaultEntry): ReactNode {
  return <span className="truncate font-medium">{entry.title || entry.filename}</span>
}

function renderStatus(entry: VaultEntry): ReactNode {
  if (!entry.status) return <span className="text-muted-foreground">—</span>
  return <Badge variant="secondary" className="text-[10px] leading-3">{entry.status}</Badge>
}

function renderPriority(entry: VaultEntry): ReactNode {
  const value = entry.properties.priority
  if (typeof value !== 'string' || !value) return <span className="text-muted-foreground">—</span>
  const variant = PRIORITY_VARIANTS[value] ?? 'outline'
  return <Badge variant={variant} className="text-[10px] leading-3">{value}</Badge>
}

function renderDateField(entry: VaultEntry, key: string, locale: AppLocale): ReactNode {
  const raw = entry.properties[key]
  if (typeof raw !== 'string' || !raw) return <span className="text-muted-foreground">—</span>
  const parsed = parseDateOrDateTime(raw)
  if (!parsed) return <span>{raw}</span>
  return <Badge variant={dueVariant(parsed)} className="text-[10px] leading-3">{formatDate(parsed, locale)}</Badge>
}

function renderEstimate(entry: VaultEntry): ReactNode {
  const value = entry.properties.estimate
  if (typeof value !== 'number') return <span className="text-muted-foreground">—</span>
  return <Badge variant="outline" className="text-[10px] leading-3">{value}</Badge>
}

function wikilinkLabel(raw: string): string {
  const inner = raw.startsWith('[[') && raw.endsWith(']]') ? raw.slice(2, -2) : raw
  return inner.includes('|') ? inner.slice(0, inner.indexOf('|')).trim() : inner.trim()
}

function renderRelationshipList(entry: VaultEntry, key: string): ReactNode {
  const values = entry.relationships[key]
  if (!values || values.length === 0) return <span className="text-muted-foreground">—</span>
  return <span className="truncate">{values.map(wikilinkLabel).join(', ')}</span>
}

function renderProject(entry: VaultEntry): ReactNode {
  const task = asTask(entry)
  if (!task?.project) return <span className="text-muted-foreground">—</span>
  return <span className="truncate">{task.project}</span>
}

function renderLabels(entry: VaultEntry): ReactNode {
  const raw = entry.properties.labels
  const items = Array.isArray(raw) ? raw.filter((item): item is string => typeof item === 'string') : []
  if (items.length === 0) return <span className="text-muted-foreground">—</span>
  return <span className="truncate">{items.join(', ')}</span>
}

function renderFileField(entry: VaultEntry, name: string, locale: AppLocale): ReactNode {
  switch (name) {
    case 'file.name':
      return <span className="truncate">{entry.title || entry.filename.replace(/\.[^.]+$/, '')}</span>
    case 'file.path':
      return <span className="truncate text-muted-foreground">{entry.path}</span>
    case 'file.folder': {
      const parts = entry.path.split('/').slice(0, -1).join('/')
      return <span className="truncate text-muted-foreground">{parts || '—'}</span>
    }
    case 'file.ext': {
      const match = /\.([^.]+)$/.exec(entry.filename)
      return <span>{match ? match[1] : '—'}</span>
    }
    case 'file.size':
      return <span>{entry.fileSize}</span>
    case 'file.mtime':
      return renderTimestamp(entry.modifiedAt, locale)
    case 'file.ctime':
      return renderTimestamp(entry.createdAt, locale)
    case 'file.tags':
      return entry.belongsTo.length === 0
        ? <span className="text-muted-foreground">—</span>
        : <span className="truncate">{entry.belongsTo.map(wikilinkLabel).join(', ')}</span>
    default:
      return <span className="text-muted-foreground">—</span>
  }
}

function renderTimestamp(timestamp: number | null, locale: AppLocale): ReactNode {
  if (!timestamp) return <span className="text-muted-foreground">—</span>
  return <span className="text-muted-foreground">{new Date(timestamp).toLocaleString(locale, { month: 'short', day: 'numeric' })}</span>
}

function renderFallbackProperty(entry: VaultEntry, key: string): ReactNode {
  const value = entry.properties[key]
  if (value === null || value === undefined) return <span className="text-muted-foreground">—</span>
  if (Array.isArray(value)) return <span className="truncate">{value.join(', ')}</span>
  return <span className="truncate">{String(value)}</span>
}

export function renderColumnCell(entry: VaultEntry, name: string, locale: AppLocale): ReactNode {
  const field = stripNotePrefix(name)
  if (field === 'title') return renderTitle(entry)
  if (field === 'status') return renderStatus(entry)
  if (field === 'priority') return renderPriority(entry)
  if (field === 'due' || field === 'start' || field === 'completed') return renderDateField(entry, field, locale)
  if (field === 'estimate') return renderEstimate(entry)
  if (field === 'project') return renderProject(entry)
  if (field === 'labels') return renderLabels(entry)
  if (field === 'assignees') return renderRelationshipList(entry, 'assignee')
  if (field === 'blocked_by' || field === 'blockedBy') return renderRelationshipList(entry, 'blocked_by')
  if (field.startsWith('file.')) return renderFileField(entry, field, locale)
  return renderFallbackProperty(entry, field)
}
