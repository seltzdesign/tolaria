import { useMemo } from 'react'
import type { VaultEntry } from '../../types'
import {
  formatDateOrDateTime,
  type DateOrDateTime,
} from '../../lib/tasks/dateOrDateTime'
import { asProject } from '../../lib/tasks/projectView'
import { TaskView } from '../../lib/tasks/taskView'
import { trackEvent } from '../../lib/telemetry'
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

export function TaskHeader({ entry, entries, onUpdate }: TaskHeaderProps) {
  const task = useMemo(() => new TaskView(entry), [entry])
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
      <StatusPillCell value={task.status} options={projectStatuses} onChange={handleStatus} />
      <PriorityCell value={task.priority} onChange={handlePriority} />
      <DateCell label="Due" value={task.due} onChange={handleDate('due')} />
      <DateCell label="Start" value={task.start} onChange={handleDate('start')} />
      <DateCell label="Done" value={task.completed} onChange={handleDate('completed')} />
      <EstimateCell value={task.estimate} onChange={handleEstimate} />
      <ChipListCell
        label="Labels"
        values={task.labels}
        onChange={handleLabels}
        placeholder="Add label"
      />
      <ProjectCell value={task.project} onChange={handleProject} />
      <ChipListCell
        label="Assignees"
        values={task.assignees}
        onChange={handleAssignees}
        placeholder="Add assignee"
      />
      <ChipListCell
        label="Blocked by"
        values={task.blockedBy}
        onChange={handleBlockedBy}
        placeholder="Add task"
      />
    </header>
  )
}
