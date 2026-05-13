import { memo, useCallback, useMemo, type KeyboardEvent } from 'react'
import { cn } from '@/lib/utils'
import type { VaultEntry, ViewFile, ViewDefinition } from '../../types'
import { translate, type AppLocale } from '../../lib/i18n'
import {
  getSortComparator,
  parseSortConfig,
  serializeSortConfig,
  getDefaultDirection,
  type SortConfig,
  type SortOption,
} from '../../utils/noteListHelpers'
import { getColumnLabel, getColumnSortOption, renderColumnCell } from './columnCell'

const DEFAULT_COLUMNS: string[] = ['title', 'status', 'priority', 'due']

type ViewDefinitionPatch = Partial<ViewDefinition>

export interface TaskTableProps {
  view: ViewFile
  filteredEntries: VaultEntry[]
  selectedEntryPath?: string | null
  onSelectNote?: (entry: VaultEntry) => void
  onUpdateViewDefinition?: (filename: string, patch: ViewDefinitionPatch, rootPath?: string) => void
  locale: AppLocale
}

function resolveColumns(view: ViewFile): string[] {
  const columns = view.definition.columns
  return columns && columns.length > 0 ? columns : DEFAULT_COLUMNS
}

function sortedEntries(entries: VaultEntry[], sortConfig: SortConfig | null): VaultEntry[] {
  if (!sortConfig) return entries
  const comparator = getSortComparator(sortConfig.option, sortConfig.direction)
  return [...entries].sort(comparator)
}

function nextSortForColumn(currentSort: SortConfig | null, target: SortOption): SortConfig {
  if (currentSort && currentSort.option === target) {
    return { option: target, direction: currentSort.direction === 'asc' ? 'desc' : 'asc' }
  }
  return { option: target, direction: getDefaultDirection(target) }
}

function sortIndicator(currentSort: SortConfig | null, target: SortOption | null): string {
  if (!target || !currentSort || currentSort.option !== target) return ''
  return currentSort.direction === 'asc' ? ' ▲' : ' ▼'
}

function handleRowKeyDown(event: KeyboardEvent<HTMLTableRowElement>, entry: VaultEntry, onSelect?: (entry: VaultEntry) => void) {
  if (!onSelect) return
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault()
    onSelect(entry)
  }
}

interface HeaderProps {
  columns: string[]
  currentSort: SortConfig | null
  locale: AppLocale
  onClickHeader: (column: string) => void
}

function TableHeader({ columns, currentSort, locale, onClickHeader }: HeaderProps) {
  return (
    <thead className="sticky top-0 z-10 bg-muted/60 text-[11px] uppercase tracking-wide text-muted-foreground">
      <tr>
        {columns.map((column) => {
          const sortable = getColumnSortOption(column) !== null
          return (
            <th
              key={column}
              data-testid={`task-table-header-${column}`}
              scope="col"
              className={cn('select-none border-b border-border px-3 py-2 text-left font-semibold', sortable ? 'cursor-pointer hover:text-foreground' : null)}
              onClick={sortable ? () => onClickHeader(column) : undefined}
            >
              {getColumnLabel(column, locale)}{sortIndicator(currentSort, getColumnSortOption(column))}
            </th>
          )
        })}
      </tr>
    </thead>
  )
}

export const TaskTable = memo(function TaskTable({
  view,
  filteredEntries,
  selectedEntryPath,
  onSelectNote,
  onUpdateViewDefinition,
  locale,
}: TaskTableProps) {
  const columns = useMemo(() => resolveColumns(view), [view])
  const currentSort = useMemo(() => parseSortConfig(view.definition.sort), [view.definition.sort])
  const rows = useMemo(() => sortedEntries(filteredEntries, currentSort), [filteredEntries, currentSort])

  const handleClickHeader = useCallback(
    (column: string) => {
      if (!onUpdateViewDefinition) return
      const target = getColumnSortOption(column)
      if (!target) return
      const next = nextSortForColumn(currentSort, target)
      onUpdateViewDefinition(view.filename, { sort: serializeSortConfig(next) }, view.rootPath)
    },
    [currentSort, onUpdateViewDefinition, view.filename, view.rootPath],
  )

  if (filteredEntries.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        {translate(locale, 'tasks.table.emptyView')}
      </div>
    )
  }

  return (
    <div data-testid="task-table" className="h-full overflow-auto">
      <table className="min-w-full border-collapse text-sm">
        <TableHeader columns={columns} currentSort={currentSort} locale={locale} onClickHeader={handleClickHeader} />
        <tbody>
          {rows.map((entry) => (
            <tr
              key={entry.path}
              role="button"
              tabIndex={0}
              data-testid="task-table-row"
              data-entry-path={entry.path}
              className={cn(
                'cursor-pointer border-b border-border/60 hover:bg-accent/30 focus:outline-none focus-visible:bg-accent/50',
                selectedEntryPath === entry.path ? 'bg-accent/40' : null,
              )}
              onClick={() => onSelectNote?.(entry)}
              onKeyDown={(event) => handleRowKeyDown(event, entry, onSelectNote)}
            >
              {columns.map((column) => (
                <td key={column} className="max-w-[16rem] truncate px-3 py-1.5 align-middle">
                  {renderColumnCell(entry, column, locale)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
})
