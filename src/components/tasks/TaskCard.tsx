import { memo, type CSSProperties, type HTMLAttributes, type KeyboardEvent } from 'react'
import { cn } from '@/lib/utils'
import { Badge } from '../ui/badge'
import type { VaultEntry } from '../../types'
import type { AppLocale } from '../../lib/i18n'
import { translate } from '../../lib/i18n'
import { asTask } from '../../lib/tasks/taskView'
import { hasTime, type DateOrDateTime } from '../../lib/tasks/dateOrDateTime'

const PRIORITY_VARIANTS: Record<string, 'destructive' | 'default' | 'secondary' | 'outline'> = {
  P0: 'destructive',
  P1: 'default',
  P2: 'secondary',
  P3: 'outline',
}

export interface TaskCardProps {
  entry: VaultEntry
  isSelected?: boolean
  onSelect?: (entry: VaultEntry) => void
  locale: AppLocale
  isDragging?: boolean
  dragHandleProps?: HTMLAttributes<HTMLElement>
  style?: CSSProperties
  innerRef?: (node: HTMLElement | null) => void
}

function PriorityChip({ value }: { value: string }) {
  const variant = PRIORITY_VARIANTS[value] ?? 'outline'
  return <Badge variant={variant} className="text-[10px] leading-3">{value}</Badge>
}

function dueLabel(due: DateOrDateTime, locale: AppLocale): string {
  if (!hasTime(due)) {
    const [year, month, day] = due.date.split('-').map(Number)
    const local = new Date(year, month - 1, day)
    return local.toLocaleDateString(locale, { month: 'short', day: 'numeric' })
  }
  const parsed = new Date(due.iso)
  return parsed.toLocaleString(locale, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })
}

function dueVariant(due: DateOrDateTime): 'destructive' | 'default' | 'secondary' {
  const todayKey = new Date().toISOString().slice(0, 10)
  const dueKey = due.kind === 'date' ? due.date : due.iso.slice(0, 10)
  if (dueKey < todayKey) return 'destructive'
  if (dueKey === todayKey) return 'default'
  return 'secondary'
}

function CardChips({ entry, locale }: { entry: VaultEntry; locale: AppLocale }) {
  const task = asTask(entry)
  if (!task) return null
  const due = task.due
  return (
    <div className="mt-1 flex flex-wrap items-center gap-1">
      {task.priority ? <PriorityChip value={task.priority} /> : null}
      {due ? (
        <Badge variant={dueVariant(due)} className="text-[10px] leading-3">
          {dueLabel(due, locale)}
        </Badge>
      ) : null}
      {task.assignees.length > 0 ? (
        <Badge variant="outline" className="text-[10px] leading-3">
          {translate(locale, 'tasks.card.assignees', { count: task.assignees.length })}
        </Badge>
      ) : null}
    </div>
  )
}

function handleKeyDown(event: KeyboardEvent<HTMLDivElement>, entry: VaultEntry, onSelect?: (entry: VaultEntry) => void) {
  if (!onSelect) return
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault()
    onSelect(entry)
  }
}

export const TaskCard = memo(function TaskCard({
  entry,
  isSelected = false,
  onSelect,
  locale,
  isDragging = false,
  dragHandleProps,
  style,
  innerRef,
}: TaskCardProps) {
  const title = entry.title || entry.filename
  return (
    <div
      ref={innerRef}
      role="button"
      tabIndex={0}
      data-testid="task-card"
      data-entry-path={entry.path}
      className={cn(
        'group cursor-grab rounded-md border border-border bg-card px-2.5 py-2 text-sm shadow-sm transition-colors',
        'hover:bg-accent/30 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring',
        isSelected ? 'ring-2 ring-primary/60' : null,
        isDragging ? 'opacity-50' : null,
      )}
      style={style}
      onClick={() => onSelect?.(entry)}
      onKeyDown={(event) => handleKeyDown(event, entry, onSelect)}
      {...dragHandleProps}
    >
      <div className="line-clamp-2 font-medium leading-snug text-foreground">{title}</div>
      <CardChips entry={entry} locale={locale} />
    </div>
  )
})
