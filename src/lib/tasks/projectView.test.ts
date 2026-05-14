import { describe, expect, it } from 'vitest'
import type { VaultEntry } from '../../types'
import { asProject, isProjectEntry, ProjectView } from './projectView'

function baseEntry(overrides: Partial<VaultEntry> = {}): VaultEntry {
  return {
    path: '/vault/project.md',
    filename: 'project.md',
    title: 'Some project',
    isA: 'project',
    aliases: [],
    belongsTo: [],
    relatedTo: [],
    status: null,
    archived: false,
    modifiedAt: null,
    createdAt: null,
    fileSize: 0,
    snippet: '',
    wordCount: 0,
    relationships: {},
    icon: null,
    color: null,
    order: null,
    sidebarLabel: null,
    template: null,
    sort: null,
    view: null,
    visible: null,
    organized: false,
    favorite: false,
    favoriteIndex: null,
    listPropertiesDisplay: [],
    outgoingLinks: [],
    properties: {},
    hasH1: false,
    ...overrides,
  }
}

function projectFixture(): VaultEntry {
  return baseEntry({
    properties: {
      task_folder: 'Projects/X/tasks',
      statuses: ['Not started', 'In progress', 'Done'],
      default_view: 'board',
      sync_enabled: true,
      sync_interval_minutes: 10,
    },
  })
}

describe('asProject / isProjectEntry', () => {
  it('returns a ProjectView only when isA is "project"', () => {
    expect(isProjectEntry(projectFixture())).toBe(true)
    expect(asProject(projectFixture())).toBeInstanceOf(ProjectView)
    expect(asProject(baseEntry({ isA: 'task' }))).toBeNull()
    expect(asProject(baseEntry({ isA: null }))).toBeNull()
  })

  it('treats isA case-insensitively so `type: Project` notes still resolve', () => {
    expect(isProjectEntry(baseEntry({ isA: 'Project' }))).toBe(true)
    expect(isProjectEntry(baseEntry({ isA: 'PROJECT' }))).toBe(true)
    expect(asProject(baseEntry({ isA: 'Project' }))).toBeInstanceOf(ProjectView)
  })
})

describe('ProjectView accessors', () => {
  it('reads basic fields', () => {
    const project = asProject(projectFixture())!
    expect(project.taskFolder).toBe('Projects/X/tasks')
    expect(project.statuses).toEqual(['Not started', 'In progress', 'Done'])
    expect(project.defaultView).toBe('board')
    expect(project.syncEnabled).toBe(true)
    expect(project.syncIntervalMinutes).toBe(10)
  })

  it('uses explicit terminal_statuses when set', () => {
    const entry = projectFixture()
    entry.properties.terminal_statuses = ['Done', 'Cancelled']
    expect(asProject(entry)!.terminalStatuses).toEqual(['Done', 'Cancelled'])
  })

  it('defaults terminal_statuses to ["Done"] when statuses contains Done', () => {
    expect(asProject(projectFixture())!.terminalStatuses).toEqual(['Done'])
  })

  it('defaults terminal_statuses to the last status when no Done', () => {
    const entry = projectFixture()
    entry.properties.statuses = ['Open', 'In review', 'Closed']
    expect(asProject(entry)!.terminalStatuses).toEqual(['Closed'])
  })

  it('defaults sync_interval_minutes to 5', () => {
    const entry = projectFixture()
    delete entry.properties.sync_interval_minutes
    expect(asProject(entry)!.syncIntervalMinutes).toBe(5)
  })

  it('defaults link_to_issues to false', () => {
    expect(asProject(projectFixture())!.linkToIssues).toBe(false)
  })

  it('returns empty terminal_statuses when no statuses at all', () => {
    const entry = baseEntry({ isA: 'project' })
    expect(asProject(entry)!.terminalStatuses).toEqual([])
  })
})
