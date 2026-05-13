import type { ReactNode } from 'react'
import type { VaultEntry } from '../../types'
import { isTaskEntry } from '../../lib/tasks/taskView'
import { TaskHeader, type TaskUpdate } from './TaskHeader'

export interface TaskEditorProps {
  entry: VaultEntry
  entries: VaultEntry[]
  onUpdate: TaskUpdate
  children: ReactNode
}

export function TaskEditor({ entry, entries, onUpdate, children }: TaskEditorProps) {
  if (!isTaskEntry(entry)) return <>{children}</>
  return (
    <div className="task-editor flex flex-col min-h-0 flex-1" data-testid="task-editor">
      <TaskHeader entry={entry} entries={entries} onUpdate={onUpdate} />
      <div className="flex-1 min-h-0">{children}</div>
    </div>
  )
}
