import type { ReactNode } from 'react'
import type { VaultEntry } from '../../types'
import { isProjectEntry } from '../../lib/tasks/projectView'
import type { AppLocale } from '../../lib/i18n'
import { ProjectHeader, type ProjectUpdate } from './ProjectHeader'

export interface ProjectEditorProps {
  entry: VaultEntry
  onUpdate: ProjectUpdate
  locale?: AppLocale
  children: ReactNode
}

export function ProjectEditor({ entry, onUpdate, locale, children }: ProjectEditorProps) {
  if (!isProjectEntry(entry)) return <>{children}</>
  return (
    <div className="project-editor flex flex-col min-h-0 flex-1" data-testid="project-editor">
      <ProjectHeader entry={entry} onUpdate={onUpdate} locale={locale} />
      <div className="flex-1 min-h-0">{children}</div>
    </div>
  )
}
