import { memo, useCallback, useMemo, useState } from 'react'
import {
  DndContext, PointerSensor, useDraggable, useDroppable, useSensor, useSensors,
  type DragEndEvent, type DragStartEvent,
} from '@dnd-kit/core'
import { cn } from '@/lib/utils'
import type { VaultEntry, ViewFile } from '../../types'
import { translate, type AppLocale } from '../../lib/i18n'
import { trackEvent } from '../../lib/telemetry'
import { TaskCard } from './TaskCard'
import {
  BOARD_COLUMN_ID_PREFIX,
  deriveBoardColumns,
  normalizeBoardField,
  planBoardDrop,
  type BoardColumn,
} from '../../lib/tasks/boardColumns'

type FrontmatterUpdate = (
  path: string,
  key: string,
  value: string | number | boolean | string[] | null,
) => void | Promise<void>

export interface TaskBoardProps {
  view: ViewFile
  filteredEntries: VaultEntry[]
  allEntries: VaultEntry[]
  selectedEntryPath?: string | null
  onSelectNote?: (entry: VaultEntry) => void
  onUpdateFrontmatter: FrontmatterUpdate
  locale: AppLocale
}

interface BoardCardProps {
  entry: VaultEntry
  isSelected: boolean
  onSelect?: (entry: VaultEntry) => void
  locale: AppLocale
}

function BoardCard({ entry, isSelected, onSelect, locale }: BoardCardProps) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: entry.path })
  return (
    <TaskCard
      entry={entry}
      isSelected={isSelected}
      onSelect={onSelect}
      locale={locale}
      isDragging={isDragging}
      innerRef={setNodeRef}
      dragHandleProps={{ ...attributes, ...listeners }}
    />
  )
}

interface BoardColumnViewProps {
  column: BoardColumn
  selectedEntryPath?: string | null
  onSelectNote?: (entry: VaultEntry) => void
  locale: AppLocale
}

function BoardColumnView({ column, selectedEntryPath, onSelectNote, locale }: BoardColumnViewProps) {
  const { setNodeRef, isOver } = useDroppable({ id: `${BOARD_COLUMN_ID_PREFIX}${column.key}` })
  return (
    <div
      ref={setNodeRef}
      data-testid="board-column"
      data-column-key={column.key}
      data-column-is-over={isOver ? 'true' : 'false'}
      className={cn(
        'flex h-full min-w-[14rem] flex-col gap-2 rounded-md border border-border bg-muted/30 p-2',
        isOver ? 'bg-accent/40' : null,
      )}
    >
      <div className="flex items-center justify-between px-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
        <span className={column.isUnset ? 'italic text-muted-foreground/80' : undefined}>{column.label}</span>
        <span>{translate(locale, 'tasks.board.columnCount', { count: column.entries.length })}</span>
      </div>
      <div className="flex flex-col gap-2 overflow-y-auto">
        {column.entries.map((entry) => (
          <BoardCard
            key={entry.path}
            entry={entry}
            isSelected={selectedEntryPath === entry.path}
            onSelect={onSelectNote}
            locale={locale}
          />
        ))}
      </div>
    </div>
  )
}

export const TaskBoard = memo(function TaskBoard({
  view,
  filteredEntries,
  allEntries,
  selectedEntryPath,
  onSelectNote,
  onUpdateFrontmatter,
  locale,
}: TaskBoardProps) {
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 4 } }))
  const groupBy = useMemo(
    () => view.definition.groupBy ?? { property: 'status' },
    [view.definition.groupBy],
  )
  const writeField = useMemo(() => normalizeBoardField(groupBy.property), [groupBy.property])
  const unsetLabel = translate(locale, 'tasks.board.unsetColumn')

  const columns = useMemo(
    () => deriveBoardColumns(filteredEntries, allEntries, groupBy, unsetLabel),
    [filteredEntries, allEntries, groupBy, unsetLabel],
  )

  const [draggingPath, setDraggingPath] = useState<string | null>(null)

  const handleDragStart = useCallback((event: DragStartEvent) => {
    setDraggingPath(typeof event.active.id === 'string' ? event.active.id : null)
  }, [])

  const handleDragEnd = useCallback(
    (event: DragEndEvent) => {
      setDraggingPath(null)
      const plan = planBoardDrop({
        filteredEntries,
        columns,
        writeField,
        activeId: event.active.id,
        overId: event.over?.id ?? null,
      })
      if (!plan) return
      trackEvent('task_status_changed', { property: plan.field, from: plan.from ?? '', to: plan.to ?? '' })
      void onUpdateFrontmatter(plan.path, plan.field, plan.to)
    },
    [filteredEntries, columns, writeField, onUpdateFrontmatter],
  )

  if (filteredEntries.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        {translate(locale, 'tasks.board.emptyView')}
      </div>
    )
  }

  return (
    <DndContext sensors={sensors} onDragStart={handleDragStart} onDragEnd={handleDragEnd}>
      <div
        data-testid="task-board"
        data-dragging-path={draggingPath ?? ''}
        className="flex h-full gap-3 overflow-x-auto p-2"
      >
        {columns.map((column) => (
          <BoardColumnView
            key={column.key || '__unset__'}
            column={column}
            selectedEntryPath={selectedEntryPath}
            onSelectNote={onSelectNote}
            locale={locale}
          />
        ))}
      </div>
    </DndContext>
  )
})
