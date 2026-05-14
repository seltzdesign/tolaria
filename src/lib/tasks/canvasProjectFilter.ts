import { useCallback, useEffect, useState } from 'react'
import type { VaultEntry } from '../../types'
import { resolveEntry, wikilinkTarget } from '../../utils/wikilink'
import { isProjectEntry } from './projectView'

const STORAGE_PREFIX = 'tolaria:canvas-project-filter:'

export interface CanvasProjectOption {
  path: string
  title: string
}

export function listCanvasProjectOptions(entries: VaultEntry[]): CanvasProjectOption[] {
  return entries
    .filter((entry) => isProjectEntry(entry) && !entry.archived)
    .map((entry) => ({ path: entry.path, title: entry.title || entry.filename }))
    .sort((a, b) => a.title.localeCompare(b.title))
}

export function filterEntriesByProject(
  entries: VaultEntry[],
  projectPath: string | null,
  allEntries: VaultEntry[],
): VaultEntry[] {
  if (!projectPath) return entries
  return entries.filter((entry) => taskBelongsToProject(entry, projectPath, allEntries))
}

function taskBelongsToProject(entry: VaultEntry, projectPath: string, allEntries: VaultEntry[]): boolean {
  const projectLinks = projectRelationshipLinks(entry)
  if (projectLinks.length === 0) return false
  return projectLinks.some((link) => resolveEntry(allEntries, wikilinkTarget(link), entry)?.path === projectPath)
}

function projectRelationshipLinks(entry: VaultEntry): string[] {
  const key = Object.keys(entry.relationships).find((k) => k.toLowerCase() === 'project')
  if (!key) return []
  return entry.relationships[key] ?? []
}

function storageKeyFor(viewFilename: string): string {
  return `${STORAGE_PREFIX}${viewFilename}`
}

function readStoredProject(viewFilename: string): string | null {
  if (typeof localStorage === 'undefined') return null
  const raw = localStorage.getItem(storageKeyFor(viewFilename))
  return raw && raw.length > 0 ? raw : null
}

export function useCanvasProjectFilter(viewFilename: string) {
  const [projectPath, setProjectPath] = useState<string | null>(() => readStoredProject(viewFilename))

  useEffect(() => {
    setProjectPath(readStoredProject(viewFilename))
  }, [viewFilename])

  const setAndPersist = useCallback((next: string | null) => {
    setProjectPath(next)
    if (typeof localStorage === 'undefined') return
    if (next) localStorage.setItem(storageKeyFor(viewFilename), next)
    else localStorage.removeItem(storageKeyFor(viewFilename))
  }, [viewFilename])

  return { projectPath, setProjectPath: setAndPersist }
}
