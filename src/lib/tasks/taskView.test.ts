import { describe, expect, it } from 'vitest'
import type { VaultEntry } from '../../types'
import { asTask, isTaskEntry, TaskView } from './taskView'

function baseEntry(overrides: Partial<VaultEntry> = {}): VaultEntry {
  return {
    path: '/vault/task.md',
    filename: 'task.md',
    title: 'Some task',
    isA: 'task',
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

function taskFixture(): VaultEntry {
  return baseEntry({
    status: 'In progress',
    properties: {
      priority: 'P1',
      due: '2026-05-20',
      start: '2026-05-15',
      estimate: 3,
      labels: ['bug', 'frontend'],
      github_sync_status: 'synced',
      github_item_node_id: 'PVTI_lAHO',
    },
    relationships: {
      project: ['[[My Cool Project]]'],
      assignee: ['[[Armin]]', '[[Bob|Bobby]]'],
      blocked_by: ['[[Set up CI]]'],
    },
  })
}

describe('asTask / isTaskEntry', () => {
  it('returns a TaskView only when isA is "task"', () => {
    const task = taskFixture()
    expect(isTaskEntry(task)).toBe(true)
    expect(asTask(task)).toBeInstanceOf(TaskView)

    const project = baseEntry({ isA: 'project' })
    expect(isTaskEntry(project)).toBe(false)
    expect(asTask(project)).toBeNull()

    const plain = baseEntry({ isA: null })
    expect(asTask(plain)).toBeNull()
  })
})

describe('TaskView accessors', () => {
  it('reads scalar properties', () => {
    const task = asTask(taskFixture())!
    expect(task.status).toBe('In progress')
    expect(task.priority).toBe('P1')
    expect(task.estimate).toBe(3)
    expect(task.githubSyncStatus).toBe('synced')
    expect(task.githubItemNodeId).toBe('PVTI_lAHO')
  })

  it('parses due/start dates and leaves completed null when absent', () => {
    const task = asTask(taskFixture())!
    expect(task.due).toEqual({ kind: 'date', date: '2026-05-20' })
    expect(task.start).toEqual({ kind: 'date', date: '2026-05-15' })
    expect(task.completed).toBeNull()
  })

  it('returns labels as a list', () => {
    const task = asTask(taskFixture())!
    expect(task.labels).toEqual(['bug', 'frontend'])
  })

  it('extracts wikilink targets from relationship fields', () => {
    const task = asTask(taskFixture())!
    expect(task.project).toBe('My Cool Project')
    expect(task.assignees).toEqual(['Armin', 'Bob'])
    expect(task.blockedBy).toEqual(['Set up CI'])
  })

  it('returns null for invalid date property', () => {
    const entry = taskFixture()
    entry.properties.due = 'not-a-date'
    const task = asTask(entry)!
    expect(task.due).toBeNull()
  })

  it('returns empty lists when relationships are absent', () => {
    const task = asTask(baseEntry({ isA: 'task' }))!
    expect(task.assignees).toEqual([])
    expect(task.blockedBy).toEqual([])
    expect(task.labels).toEqual([])
    expect(task.project).toBeNull()
  })

  it('returns null estimate when value is non-numeric', () => {
    const entry = taskFixture()
    entry.properties.estimate = 'three'
    const task = asTask(entry)!
    expect(task.estimate).toBeNull()
  })
})
