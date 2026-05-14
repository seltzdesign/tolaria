import { beforeEach, describe, expect, it } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import type { VaultEntry } from '../../types'
import {
  filterEntriesByProject,
  listCanvasProjectOptions,
  useCanvasProjectFilter,
} from './canvasProjectFilter'

const localStorageMock = (() => {
  let store: Record<string, string> = {}
  return {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, value: string) => { store[key] = value },
    removeItem: (key: string) => { delete store[key] },
    clear: () => { store = {} },
  }
})()

Object.defineProperty(globalThis, 'localStorage', { value: localStorageMock, writable: true })

function makeEntry(overrides: Partial<VaultEntry> = {}): VaultEntry {
  return {
    path: '/vault/entry.md',
    filename: 'entry.md',
    title: 'Entry',
    isA: null,
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

describe('listCanvasProjectOptions', () => {
  it('returns all active project entries sorted by title', () => {
    const entries = [
      makeEntry({ path: '/b.md', filename: 'b.md', title: 'Beta', isA: 'project' }),
      makeEntry({ path: '/a.md', filename: 'a.md', title: 'Alpha', isA: 'project' }),
      makeEntry({ path: '/n.md', filename: 'n.md', title: 'Note', isA: 'note' }),
    ]

    expect(listCanvasProjectOptions(entries)).toEqual([
      { path: '/a.md', title: 'Alpha' },
      { path: '/b.md', title: 'Beta' },
    ])
  })

  it('excludes archived projects', () => {
    const entries = [
      makeEntry({ path: '/a.md', filename: 'a.md', title: 'Active', isA: 'project' }),
      makeEntry({ path: '/b.md', filename: 'b.md', title: 'Archived', isA: 'project', archived: true }),
    ]
    expect(listCanvasProjectOptions(entries).map((p) => p.title)).toEqual(['Active'])
  })

  it('matches isA case-insensitively so `type: Project` is included', () => {
    const entries = [
      makeEntry({ path: '/a.md', filename: 'a.md', title: 'Capital P', isA: 'Project' }),
    ]
    expect(listCanvasProjectOptions(entries)).toEqual([{ path: '/a.md', title: 'Capital P' }])
  })
})

describe('filterEntriesByProject', () => {
  const project = makeEntry({ path: '/vault/q2-launch.md', filename: 'q2-launch.md', title: 'Q2 Launch', isA: 'project' })
  const otherProject = makeEntry({ path: '/vault/q3.md', filename: 'q3.md', title: 'Q3', isA: 'project' })
  const taskA = makeEntry({
    path: '/vault/task-a.md',
    filename: 'task-a.md',
    title: 'Task A',
    isA: 'task',
    relationships: { project: ['[[q2-launch]]'] },
  })
  const taskB = makeEntry({
    path: '/vault/task-b.md',
    filename: 'task-b.md',
    title: 'Task B',
    isA: 'task',
    relationships: { project: ['[[Q2 Launch]]'] }, // by title
  })
  const taskC = makeEntry({
    path: '/vault/task-c.md',
    filename: 'task-c.md',
    title: 'Task C',
    isA: 'task',
    relationships: { project: ['[[q3]]'] },
  })
  const taskD = makeEntry({
    path: '/vault/task-d.md',
    filename: 'task-d.md',
    title: 'Task D',
    isA: 'task',
    relationships: {},
  })

  const allEntries = [project, otherProject, taskA, taskB, taskC, taskD]
  const tasks = [taskA, taskB, taskC, taskD]

  it('returns entries unchanged when projectPath is null', () => {
    expect(filterEntriesByProject(tasks, null, allEntries)).toEqual(tasks)
  })

  it('keeps tasks whose project wikilink resolves to the selected project (by filename)', () => {
    expect(filterEntriesByProject(tasks, project.path, allEntries).map((e) => e.title)).toEqual(['Task A', 'Task B'])
  })

  it('drops tasks with no project relationship', () => {
    expect(filterEntriesByProject(tasks, project.path, allEntries).some((e) => e.title === 'Task D')).toBe(false)
  })

  it('drops tasks pointing at a different project', () => {
    expect(filterEntriesByProject(tasks, project.path, allEntries).some((e) => e.title === 'Task C')).toBe(false)
  })
})

describe('useCanvasProjectFilter', () => {
  beforeEach(() => {
    localStorage.clear()
  })

  it('starts as null when nothing is persisted', () => {
    const { result } = renderHook(() => useCanvasProjectFilter('task-board.yml'))
    expect(result.current.projectPath).toBeNull()
  })

  it('persists project selection per view filename', () => {
    const { result } = renderHook(() => useCanvasProjectFilter('task-board.yml'))
    act(() => result.current.setProjectPath('/vault/q2-launch.md'))
    expect(localStorage.getItem('tolaria:canvas-project-filter:task-board.yml')).toBe('/vault/q2-launch.md')
  })

  it('clears the stored value when set to null', () => {
    localStorage.setItem('tolaria:canvas-project-filter:task-board.yml', '/vault/q2-launch.md')
    const { result } = renderHook(() => useCanvasProjectFilter('task-board.yml'))
    expect(result.current.projectPath).toBe('/vault/q2-launch.md')
    act(() => result.current.setProjectPath(null))
    expect(localStorage.getItem('tolaria:canvas-project-filter:task-board.yml')).toBeNull()
    expect(result.current.projectPath).toBeNull()
  })

  it('reloads the stored value when the view filename changes', () => {
    localStorage.setItem('tolaria:canvas-project-filter:task-board.yml', '/vault/a.md')
    localStorage.setItem('tolaria:canvas-project-filter:task-timeline.yml', '/vault/b.md')
    const { result, rerender } = renderHook(({ filename }) => useCanvasProjectFilter(filename), {
      initialProps: { filename: 'task-board.yml' },
    })
    expect(result.current.projectPath).toBe('/vault/a.md')
    rerender({ filename: 'task-timeline.yml' })
    expect(result.current.projectPath).toBe('/vault/b.md')
  })
})
