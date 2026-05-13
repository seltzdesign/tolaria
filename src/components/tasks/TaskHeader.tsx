import { useMemo } from 'react'
import type { VaultEntry } from '../../types'
import {
  formatDateOrDateTime,
  type DateOrDateTime,
} from '../../lib/tasks/dateOrDateTime'
import { asProject } from '../../lib/tasks/projectView'
import { TaskView } from '../../lib/tasks/taskView'
import { trackEvent } from '../../lib/telemetry'
import { createTranslator, type AppLocale } from '../../lib/i18n'
import { ChipListCell } from './cells/ChipListCell'
import { DateCell } from './cells/DateCell'
import { EstimateCell } from './cells/EstimateCell'
import { PriorityCell } from './cells/PriorityCell'
import { ProjectCell } from './cells/ProjectCell'
import { StatusPillCell } from './cells/StatusPillCell'

export type TaskUpdate = (key: string, value: TaskPropertyValue) => void
type TaskPropertyValue = string | number | boolean | string[] | null

type TaskTelemetryProperty =
  | 'status'
  | 'priority'
  | 'due'
  | 'start'
  | 'completed'
  | 'assignee'
  | 'project'
  | 'labels'
  | 'estimate'
  | 'blocked_by'

export interface TaskHeaderProps {
  entry: VaultEntry
  entries: VaultEntry[]
  onUpdate: TaskUpdate
  locale?: AppLocale
}

function wikilinkArray(targets: string[]): string[] {
  return targets.map((target) => `[[${target}]]`)
}

function statusesForProject(
  entries: readonly VaultEntry[],
  projectTitle: string | null,
): readonly string[] {
  if (!projectTitle) return []
  const projectEntry = entries.find(
    (entry) => entry.isA === 'project' && entry.title === projectTitle,
  )
  if (!projectEntry) return []
  return asProject(projectEntry)?.statuses ?? []
}

function trackPropertyEdit(property: TaskTelemetryProperty): void {
  trackEvent('task_property_edited', { property })
}

export function TaskHeader({ entry, entries, onUpdate, locale = 'en' }: TaskHeaderProps) {
  const task = useMemo(() => new TaskView(entry), [entry])
  const t = useMemo(() => createTranslator(locale), [locale])
  const projectStatuses = useMemo(
    () => statusesForProject(entries, task.project),
    [entries, task.project],
  )

  const handleStatus = (next: string | null) => {
    onUpdate('status', next)
    trackPropertyEdit('status')
  }
  const handlePriority = (next: string | null) => {
    onUpdate('priority', next)
    trackPropertyEdit('priority')
  }
  const handleDate = (key: TaskTelemetryProperty) => (next: DateOrDateTime | null) => {
    onUpdate(key, next ? formatDateOrDateTime(next) : null)
    trackPropertyEdit(key)
  }
  const handleEstimate = (next: number | null) => {
    onUpdate('estimate', next)
    trackPropertyEdit('estimate')
  }
  const handleLabels = (next: string[]) => {
    onUpdate('labels', next.length > 0 ? next : null)
    trackPropertyEdit('labels')
  }
  const handleAssignees = (next: string[]) => {
    onUpdate('assignee', next.length > 0 ? wikilinkArray(next) : null)
    trackPropertyEdit('assignee')
  }
  const handleBlockedBy = (next: string[]) => {
    onUpdate('blocked_by', next.length > 0 ? wikilinkArray(next) : null)
    trackPropertyEdit('blocked_by')
  }
  const handleProject = (next: string | null) => {
    onUpdate('project', next ? `[[${next}]]` : null)
    trackPropertyEdit('project')
  }

  return (
    <header
      className="flex flex-wrap items-center gap-3 border-b px-4 py-3"
      data-testid="task-header"
    >
      <StatusPillCell
        value={task.status}
        options={projectStatuses}
        onChange={handleStatus}
        placeholder={t('tasks.cell.status')}
      />
      <PriorityCell value={task.priority} onChange={handlePriority} placeholder={t('tasks.cell.priority')} />
      <DateCell
        label={t('tasks.cell.due')}
        value={task.due}
        onChange={handleDate('due')}
        clearLabel={t('tasks.cell.clear')}
      />
      <DateCell
        label={t('tasks.cell.start')}
        value={task.start}
        onChange={handleDate('start')}
        clearLabel={t('tasks.cell.clear')}
      />
      <DateCell
        label={t('tasks.cell.completed')}
        value={task.completed}
        onChange={handleDate('completed')}
        clearLabel={t('tasks.cell.clear')}
      />
      <EstimateCell value={task.estimate} onChange={handleEstimate} placeholder={t('tasks.cell.estimate')} />
      <ChipListCell
        label={t('tasks.cell.labels')}
        values={task.labels}
        onChange={handleLabels}
        placeholder={t('tasks.cell.addLabel')}
      />
      <ProjectCell value={task.project} onChange={handleProject} placeholder={t('tasks.cell.project')} />
      <ChipListCell
        label={t('tasks.cell.assignees')}
        values={task.assignees}
        onChange={handleAssignees}
        placeholder={t('tasks.cell.addAssignee')}
      />
      <ChipListCell
        label={t('tasks.cell.blockedBy')}
        values={task.blockedBy}
        onChange={handleBlockedBy}
        placeholder={t('tasks.cell.addBlockedBy')}
      />
    </header>
  )
}
