import { useCallback, useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Button } from './ui/button'
import { Input } from './ui/input'
import {
  NumberInputControl,
  SectionHeading,
  SettingsGroup,
  SettingsRow,
  SettingsSwitchRow,
} from './SettingsControls'
import type { createTranslator } from '../lib/i18n'

type Translate = ReturnType<typeof createTranslator>

interface GitHubProjectsSettingsSectionProps {
  t: Translate
  enabled: boolean
  setEnabled: (value: boolean) => void
  syncIntervalMinutes: number
  setSyncIntervalMinutes: (value: number) => void
}

type ConnectionState =
  | { kind: 'idle' }
  | { kind: 'testing' }
  | { kind: 'success'; login: string }
  | { kind: 'error'; message: string }

export function GitHubProjectsSettingsSection({
  t,
  enabled,
  setEnabled,
  syncIntervalMinutes,
  setSyncIntervalMinutes,
}: GitHubProjectsSettingsSectionProps) {
  const [pat, setPat] = useState('')
  const [hasStoredPat, setHasStoredPat] = useState(false)
  const [saving, setSaving] = useState(false)
  const [connection, setConnection] = useState<ConnectionState>({ kind: 'idle' })

  const refreshPatPresence = useCallback(async () => {
    try {
      const present = await invoke<boolean>('github_pat_present')
      setHasStoredPat(present)
    } catch {
      setHasStoredPat(false)
    }
  }, [])

  useEffect(() => {
    void refreshPatPresence()
  }, [refreshPatPresence])

  const handleSavePat = async () => {
    const trimmed = pat.trim()
    if (!trimmed) return
    setSaving(true)
    try {
      await invoke('github_set_pat', { pat: trimmed })
      setPat('')
      setConnection({ kind: 'idle' })
      await refreshPatPresence()
    } catch (error) {
      setConnection({ kind: 'error', message: errorMessage(error) })
    } finally {
      setSaving(false)
    }
  }

  const handleClearPat = async () => {
    try {
      await invoke('github_clear_pat')
      setConnection({ kind: 'idle' })
      await refreshPatPresence()
    } catch (error) {
      setConnection({ kind: 'error', message: errorMessage(error) })
    }
  }

  const handleTestConnection = async () => {
    setConnection({ kind: 'testing' })
    try {
      const login = await invoke<string>('github_test_connection')
      setConnection({ kind: 'success', login })
    } catch (error) {
      setConnection({ kind: 'error', message: errorMessage(error) })
    }
  }

  return (
    <>
      <SectionHeading title={t('settings.githubProjects.title')} />
      <SettingsGroup>
        <SettingsSwitchRow
          label={t('settings.githubProjects.enabled')}
          description={t('settings.githubProjects.enabledDescription')}
          checked={enabled}
          onChange={setEnabled}
          testId="settings-github-projects-enabled"
        />
        <SettingsRow
          label={t('settings.githubProjects.syncInterval')}
          description={t('settings.githubProjects.syncIntervalDescription')}
          controlWidth="narrow"
        >
          <NumberInputControl
            value={syncIntervalMinutes}
            onValueChange={setSyncIntervalMinutes}
            testId="settings-github-projects-sync-interval"
            ariaLabel={t('settings.githubProjects.syncInterval')}
            disabled={!enabled}
          />
        </SettingsRow>
        <SettingsRow
          label={t('settings.githubProjects.pat')}
          description={
            hasStoredPat
              ? t('settings.githubProjects.patDescriptionStored')
              : t('settings.githubProjects.patDescription')
          }
        >
          <div className="flex w-full flex-col gap-2 lg:flex-row">
            <Input
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder="ghp_..."
              value={pat}
              onChange={(event) => setPat(event.target.value)}
              data-testid="settings-github-projects-pat-input"
              className="flex-1 bg-transparent"
              disabled={!enabled}
            />
            <Button
              type="button"
              variant="default"
              size="sm"
              onClick={handleSavePat}
              disabled={!enabled || saving || pat.trim().length === 0}
              data-testid="settings-github-projects-save-pat"
            >
              {hasStoredPat ? t('settings.githubProjects.replacePat') : t('settings.githubProjects.savePat')}
            </Button>
          </div>
        </SettingsRow>
        <SettingsRow
          label={t('settings.githubProjects.testConnection')}
          description={t('settings.githubProjects.testConnectionDescription')}
        >
          <div className="flex w-full items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleTestConnection}
              disabled={!enabled || !hasStoredPat || connection.kind === 'testing'}
              data-testid="settings-github-projects-test-connection"
            >
              {connection.kind === 'testing'
                ? t('settings.githubProjects.testing')
                : t('settings.githubProjects.testConnection')}
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={handleClearPat}
              disabled={!hasStoredPat}
              data-testid="settings-github-projects-clear-pat"
            >
              {t('settings.githubProjects.clearPat')}
            </Button>
          </div>
        </SettingsRow>
        <ConnectionStatus t={t} connection={connection} />
      </SettingsGroup>
    </>
  )
}

function ConnectionStatus({ t, connection }: { t: Translate; connection: ConnectionState }) {
  if (connection.kind === 'idle' || connection.kind === 'testing') return null
  if (connection.kind === 'success') {
    return (
      <p
        className="px-3 pb-1 pt-2 text-xs font-medium text-emerald-600 dark:text-emerald-400"
        data-testid="settings-github-projects-status-success"
      >
        {t('settings.githubProjects.connectedAs')} <span className="font-mono">@{connection.login}</span>
      </p>
    )
  }
  return (
    <p
      className="px-3 pb-1 pt-2 text-xs font-medium text-destructive"
      data-testid="settings-github-projects-status-error"
    >
      {connection.message}
    </p>
  )
}

function errorMessage(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return String(error)
}
