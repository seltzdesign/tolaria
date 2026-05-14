import { useEffect, useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Button } from '../ui/button'
import { Input } from '../ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { createTranslator, type AppLocale } from '../../lib/i18n'

const NONE_VALUE = '__none__'

const LOCAL_FIELDS: { local: string; defaultGuesses: string[] }[] = [
  { local: 'status', defaultGuesses: ['Status'] },
  { local: 'priority', defaultGuesses: ['Priority'] },
  { local: 'due', defaultGuesses: ['Due', 'End date', 'Target date'] },
  { local: 'start', defaultGuesses: ['Start', 'Start date'] },
  { local: 'estimate', defaultGuesses: ['Estimate', 'Effort', 'Size'] },
  { local: 'assignee', defaultGuesses: ['Assignees', 'Owner'] },
]

interface ProjectSummary {
  id: string
  number: number
  title: string
  url: string
  closed: boolean
}

interface ProjectField {
  id: string
  name: string
  data_type: string
  options: { id: string; name: string }[]
}

interface ProjectResolution {
  project: ProjectSummary
  fields: ProjectField[]
}

interface FieldMappingEntry {
  local: string
  github: string
}

interface BindingPayload {
  project_url: string
  project_node_id: string
  sync_interval_minutes: number | null
  link_to_issues: boolean | null
  github_issue_repo: string | null
  status_field: string | null
  field_mappings: FieldMappingEntry[]
}

export interface BindGitHubProjectModalProps {
  open: boolean
  notePath: string
  initialUrl?: string | null
  alreadyBound: boolean
  locale?: AppLocale
  onClose: () => void
  onBound?: () => void
  onUnbound?: () => void
}

type Stage = { kind: 'idle' } | { kind: 'resolving' } | { kind: 'resolved'; data: ProjectResolution } | { kind: 'error'; message: string }

function defaultMappingFor(local: string, fields: ProjectField[]): string {
  const guesses = LOCAL_FIELDS.find((entry) => entry.local === local)?.defaultGuesses ?? []
  for (const guess of guesses) {
    const match = fields.find((field) => field.name.toLowerCase() === guess.toLowerCase())
    if (match) return match.name
  }
  return ''
}

function errorMessage(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return String(error)
}

export function BindGitHubProjectModal({
  open,
  notePath,
  initialUrl,
  alreadyBound,
  locale = 'en',
  onClose,
  onBound,
  onUnbound,
}: BindGitHubProjectModalProps) {
  const t = useMemo(() => createTranslator(locale), [locale])
  const [url, setUrl] = useState(initialUrl ?? '')
  const [stage, setStage] = useState<Stage>({ kind: 'idle' })
  const [mappings, setMappings] = useState<Record<string, string>>({})
  const [saving, setSaving] = useState(false)
  const [savedError, setSavedError] = useState<string | null>(null)
  const [unbindWorking, setUnbindWorking] = useState(false)

  useEffect(() => {
    if (!open) {
      setStage({ kind: 'idle' })
      setMappings({})
      setSaving(false)
      setSavedError(null)
      setUnbindWorking(false)
      setUrl(initialUrl ?? '')
    }
  }, [open, initialUrl])

  const handleResolve = async () => {
    const trimmed = url.trim()
    if (!trimmed) return
    setStage({ kind: 'resolving' })
    setSavedError(null)
    try {
      const data = await invoke<ProjectResolution>('github_resolve_project_url', {
        projectUrl: trimmed,
      })
      const seeded: Record<string, string> = {}
      for (const entry of LOCAL_FIELDS) {
        seeded[entry.local] = defaultMappingFor(entry.local, data.fields)
      }
      setMappings(seeded)
      setStage({ kind: 'resolved', data })
    } catch (error) {
      setStage({ kind: 'error', message: errorMessage(error) })
    }
  }

  const handleBind = async () => {
    if (stage.kind !== 'resolved') return
    setSaving(true)
    setSavedError(null)
    try {
      const field_mappings: FieldMappingEntry[] = Object.entries(mappings)
        .filter(([local, github]) => local !== 'status' && github.length > 0)
        .map(([local, github]) => ({ local, github }))
      const payload: BindingPayload = {
        project_url: stage.data.project.url,
        project_node_id: stage.data.project.id,
        sync_interval_minutes: null,
        link_to_issues: null,
        github_issue_repo: null,
        status_field: mappings.status || null,
        field_mappings,
      }
      await invoke('github_bind_project', { notePath, bindingInput: payload })
      onBound?.()
      onClose()
    } catch (error) {
      setSavedError(errorMessage(error))
    } finally {
      setSaving(false)
    }
  }

  const handleUnbind = async () => {
    setUnbindWorking(true)
    try {
      await invoke('github_unbind_project', { notePath })
      onUnbound?.()
      onClose()
    } catch (error) {
      setSavedError(errorMessage(error))
    } finally {
      setUnbindWorking(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(isOpen) => { if (!isOpen) onClose() }}>
      <DialogContent className="flex max-h-[80vh] flex-col sm:max-w-[560px]">
        <DialogHeader>
          <DialogTitle>{t('githubBind.title')}</DialogTitle>
          <DialogDescription>{t('githubBind.description')}</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4 overflow-auto py-2">
          <label className="flex flex-col gap-1">
            <span className="text-xs font-medium text-muted-foreground">{t('githubBind.urlLabel')}</span>
            <div className="flex gap-2">
              <Input
                placeholder="https://github.com/users/<you>/projects/<n>"
                value={url}
                onChange={(event) => setUrl(event.target.value)}
                data-testid="github-bind-url-input"
                spellCheck={false}
                className="flex-1"
              />
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handleResolve}
                disabled={stage.kind === 'resolving' || url.trim().length === 0}
                data-testid="github-bind-resolve"
              >
                {stage.kind === 'resolving' ? t('githubBind.resolving') : t('githubBind.resolve')}
              </Button>
            </div>
          </label>

          {stage.kind === 'error' && (
            <p className="text-xs font-medium text-destructive" data-testid="github-bind-error">
              {stage.message}
            </p>
          )}

          {stage.kind === 'resolved' && (
            <>
              <div
                className="rounded-md border border-border bg-[var(--surface-sidebar)] px-3 py-2 text-xs"
                data-testid="github-bind-resolved-summary"
              >
                <div className="font-medium text-foreground">{stage.data.project.title}</div>
                <div className="text-muted-foreground">
                  #{stage.data.project.number}
                  {stage.data.project.closed ? ` · ${t('githubBind.closed')}` : null}
                </div>
              </div>
              <div className="flex flex-col gap-2">
                <div className="text-xs font-medium text-muted-foreground">{t('githubBind.mappingLabel')}</div>
                <div className="grid grid-cols-[120px_1fr] items-center gap-x-3 gap-y-2">
                  {LOCAL_FIELDS.map((entry) => (
                    <FieldMappingRow
                      key={entry.local}
                      localKey={entry.local}
                      fields={stage.data.fields}
                      value={mappings[entry.local] ?? ''}
                      onChange={(next) => setMappings((current) => ({ ...current, [entry.local]: next }))}
                      label={t(`githubBind.field.${entry.local}` as never)}
                      emptyLabel={t('githubBind.field.skip')}
                    />
                  ))}
                </div>
              </div>
            </>
          )}

          {savedError && (
            <p className="text-xs font-medium text-destructive" data-testid="github-bind-save-error">
              {savedError}
            </p>
          )}
        </div>

        <DialogFooter className="flex flex-row items-center justify-between">
          <div>
            {alreadyBound && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={handleUnbind}
                disabled={unbindWorking}
                data-testid="github-bind-unbind"
              >
                {t('githubBind.unbind')}
              </Button>
            )}
          </div>
          <div className="flex gap-2">
            <Button type="button" variant="ghost" size="sm" onClick={onClose}>
              {t('githubBind.cancel')}
            </Button>
            <Button
              type="button"
              variant="default"
              size="sm"
              onClick={handleBind}
              disabled={stage.kind !== 'resolved' || saving}
              data-testid="github-bind-save"
            >
              {saving ? t('githubBind.saving') : t('githubBind.save')}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

interface FieldMappingRowProps {
  localKey: string
  fields: ProjectField[]
  value: string
  onChange: (next: string) => void
  label: string
  emptyLabel: string
}

function FieldMappingRow({ localKey, fields, value, onChange, label, emptyLabel }: FieldMappingRowProps) {
  return (
    <>
      <div className="text-xs font-medium text-foreground">{label}</div>
      <Select
        value={value === '' ? NONE_VALUE : value}
        onValueChange={(next) => onChange(next === NONE_VALUE ? '' : next)}
      >
        <SelectTrigger size="sm" className="h-8 text-xs" data-testid={`github-bind-field-${localKey}`}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent className="z-[13000]">
          <SelectItem value={NONE_VALUE} className="text-xs">
            {emptyLabel}
          </SelectItem>
          {fields.map((field) => (
            <SelectItem key={field.id} value={field.name} className="text-xs">
              {field.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </>
  )
}
