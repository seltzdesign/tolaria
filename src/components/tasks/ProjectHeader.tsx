import { useMemo, useState } from 'react'
import type { VaultEntry } from '../../types'
import { ProjectView } from '../../lib/tasks/projectView'
import { trackEvent } from '../../lib/telemetry'
import { createTranslator, type AppLocale } from '../../lib/i18n'
import { ChipListCell } from './cells/ChipListCell'
import { Input } from '../ui/input'
import { Button } from '../ui/button'
import { BindGitHubProjectModal } from './BindGitHubProjectModal'

export type ProjectUpdate = (key: string, value: ProjectPropertyValue) => void
type ProjectPropertyValue = string | number | boolean | string[] | null

type ProjectTelemetryProperty =
  | 'task_folder'
  | 'statuses'
  | 'terminal_statuses'
  | 'default_view'

export interface ProjectHeaderProps {
  entry: VaultEntry
  onUpdate: ProjectUpdate
  locale?: AppLocale
}

function trackPropertyEdit(property: ProjectTelemetryProperty): void {
  trackEvent('project_property_edited', { property })
}

function readProjectUrl(entry: VaultEntry): string | null {
  const value = entry.properties.github_project_url
  return typeof value === 'string' && value.length > 0 ? value : null
}

export function ProjectHeader({ entry, onUpdate, locale = 'en' }: ProjectHeaderProps) {
  const project = useMemo(() => new ProjectView(entry), [entry])
  const t = useMemo(() => createTranslator(locale), [locale])
  const [bindOpen, setBindOpen] = useState(false)
  const projectUrl = readProjectUrl(entry)
  const alreadyBound = projectUrl !== null

  const handleTaskFolder = (next: string) => {
    const trimmed = next.trim()
    onUpdate('task_folder', trimmed.length > 0 ? trimmed : null)
    trackPropertyEdit('task_folder')
  }
  const handleStatuses = (next: string[]) => {
    onUpdate('statuses', next.length > 0 ? next : null)
    trackPropertyEdit('statuses')
  }
  const handleTerminalStatuses = (next: string[]) => {
    onUpdate('terminal_statuses', next.length > 0 ? next : null)
    trackPropertyEdit('terminal_statuses')
  }
  const handleDefaultView = (next: string) => {
    const trimmed = next.trim()
    onUpdate('default_view', trimmed.length > 0 ? trimmed : null)
    trackPropertyEdit('default_view')
  }

  return (
    <header
      className="flex flex-wrap items-center gap-3 border-b px-4 py-3"
      data-testid="project-header"
    >
      <label className="flex items-center gap-2">
        <span className="text-muted-foreground text-xs">{t('tasks.project.taskFolder')}</span>
        <Input
          defaultValue={project.taskFolder ?? ''}
          onBlur={(event) => handleTaskFolder(event.target.value)}
          data-testid="project-task-folder-input"
          className="h-8 w-48"
        />
      </label>
      <ChipListCell
        label={t('tasks.project.statuses')}
        values={project.statuses}
        onChange={handleStatuses}
        placeholder={t('tasks.project.addStatus')}
      />
      <ChipListCell
        label={t('tasks.project.terminalStatuses')}
        values={project.terminalStatuses}
        onChange={handleTerminalStatuses}
        placeholder={t('tasks.project.addTerminal')}
      />
      <label className="flex items-center gap-2">
        <span className="text-muted-foreground text-xs">{t('tasks.project.defaultView')}</span>
        <Input
          defaultValue={project.defaultView ?? ''}
          onBlur={(event) => handleDefaultView(event.target.value)}
          data-testid="project-default-view-input"
          className="h-8 w-32"
        />
      </label>
      <Button
        type="button"
        variant={alreadyBound ? 'outline' : 'default'}
        size="sm"
        onClick={() => setBindOpen(true)}
        data-testid="project-bind-github"
      >
        {alreadyBound ? t('tasks.project.editGithubBinding') : t('tasks.project.bindGithub')}
      </Button>
      <BindGitHubProjectModal
        open={bindOpen}
        notePath={entry.path}
        initialUrl={projectUrl}
        alreadyBound={alreadyBound}
        locale={locale}
        onClose={() => setBindOpen(false)}
      />
    </header>
  )
}
