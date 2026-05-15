import { useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { VaultEntry } from '../../types'
import { ProjectView } from '../../lib/tasks/projectView'
import { trackEvent } from '../../lib/telemetry'
import { createTranslator, type AppLocale } from '../../lib/i18n'
import { ChipListCell } from './cells/ChipListCell'
import { Input } from '../ui/input'
import { Button } from '../ui/button'
import { BindGitHubProjectModal } from './BindGitHubProjectModal'

interface PullResult {
  created: number
  updated: number
  deleted: number
  unchanged: number
  items_seen: number
  items_skipped: number
  errors: string[]
}

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
  vaultPath?: string | null
}

type SyncState =
  | { kind: 'idle' }
  | { kind: 'running' }
  | { kind: 'success'; result: PullResult }
  | { kind: 'error'; message: string }

function trackPropertyEdit(property: ProjectTelemetryProperty): void {
  trackEvent('project_property_edited', { property })
}

function readProjectUrl(entry: VaultEntry): string | null {
  const value = entry.properties.github_project_url
  return typeof value === 'string' && value.length > 0 ? value : null
}

export function ProjectHeader({
  entry,
  onUpdate,
  locale = 'en',
  vaultPath,
}: ProjectHeaderProps) {
  const project = useMemo(() => new ProjectView(entry), [entry])
  const t = useMemo(() => createTranslator(locale), [locale])
  const [bindOpen, setBindOpen] = useState(false)
  const [syncState, setSyncState] = useState<SyncState>({ kind: 'idle' })
  const projectUrl = readProjectUrl(entry)
  const alreadyBound = projectUrl !== null
  const canSync = alreadyBound && typeof vaultPath === 'string' && vaultPath.length > 0

  const handleSync = async () => {
    if (!canSync) return
    setSyncState({ kind: 'running' })
    try {
      const result = await invoke<PullResult>('github_sync_pull', {
        vaultPath,
        notePath: entry.path,
      })
      setSyncState({ kind: 'success', result })
      trackEvent('github_project_sync_pulled', {
        created: result.created,
        updated: result.updated,
        deleted: result.deleted,
        unchanged: result.unchanged,
        had_errors: result.errors.length > 0,
      })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      setSyncState({ kind: 'error', message })
    }
  }

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
      {alreadyBound && (
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={!canSync || syncState.kind === 'running'}
          onClick={() => {
            void handleSync()
          }}
          data-testid="project-sync-github"
        >
          {syncState.kind === 'running'
            ? t('tasks.project.syncing')
            : t('tasks.project.syncNow')}
        </Button>
      )}
      {syncState.kind === 'success' && (
        <span
          className="text-muted-foreground text-xs"
          data-testid="project-sync-result"
        >
          {t('tasks.project.syncResult', {
            created: syncState.result.created,
            updated: syncState.result.updated,
            deleted: syncState.result.deleted,
            unchanged: syncState.result.unchanged,
            items_seen: syncState.result.items_seen,
            items_skipped: syncState.result.items_skipped,
          })}
        </span>
      )}
      {syncState.kind === 'error' && (
        <span
          className="text-destructive text-xs"
          data-testid="project-sync-error"
        >
          {t('tasks.project.syncFailed', { message: syncState.message })}
        </span>
      )}
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
