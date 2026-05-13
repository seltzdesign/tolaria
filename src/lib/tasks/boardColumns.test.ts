import { describe, expect, it } from 'vitest'
import type { VaultEntry } from '../../types'
import { deriveBoardColumns, normalizeBoardField } from './boardColumns'

function entry(overrides: Partial<VaultEntry> = {}): VaultEntry {
  return {
    path: `/vault/${overrides.filename ?? 'note.md'}`,
    filename: 'note.md',
    title: 'Note',
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

describe('normalizeBoardField', () => {
  it('strips the note. prefix', () => {
    expect(normalizeBoardField('note.status')).toBe('status')
    expect(normalizeBoardField('note.priority')).toBe('priority')
  })

  it('returns the bare field unchanged', () => {
    expect(normalizeBoardField('status')).toBe('status')
    expect(normalizeBoardField('priority')).toBe('priority')
  })
})

describe('deriveBoardColumns', () => {
  it('returns distinct status values when no project binding is present', () => {
    const entries = [
      entry({ path: '/a.md', status: 'In progress' }),
      entry({ path: '/b.md', status: 'Backlog' }),
      entry({ path: '/c.md', status: 'In progress' }),
    ]
    const columns = deriveBoardColumns(entries, entries, { property: 'status' })
    expect(columns.map((column) => column.key)).toEqual(['In progress', 'Backlog'])
    expect(columns[0].entries).toHaveLength(2)
    expect(columns[1].entries).toHaveLength(1)
    expect(columns.some((column) => column.isUnset)).toBe(false)
  })

  it('prepends a (unset) column when any entry has no value for the field', () => {
    const entries = [
      entry({ path: '/a.md', status: 'In progress' }),
      entry({ path: '/b.md', status: null }),
    ]
    const columns = deriveBoardColumns(entries, entries, { property: 'status' })
    expect(columns.map((column) => column.label)).toContain('(unset)')
    const unset = columns.find((column) => column.isUnset)
    expect(unset?.entries.map((e) => e.path)).toEqual(['/b.md'])
  })

  it('uses project statuses verbatim when a bound project is found', () => {
    const project = entry({
      path: '/Q2.md',
      title: 'Q2 Launch',
      isA: 'project',
      properties: { statuses: ['Backlog', 'Shipping'] },
    })
    const taskA = entry({
      path: '/a.md',
      status: 'Shipping',
      relationships: { project: ['[[Q2 Launch]]'] },
    })
    const taskB = entry({
      path: '/b.md',
      status: 'Backlog',
      relationships: { project: ['[[Q2 Launch]]'] },
    })
    const columns = deriveBoardColumns([taskA, taskB], [project, taskA, taskB], { property: 'status' })
    expect(columns.map((column) => column.key)).toEqual(['Backlog', 'Shipping'])
    expect(columns[0].entries.map((e) => e.path)).toEqual(['/b.md'])
    expect(columns[1].entries.map((e) => e.path)).toEqual(['/a.md'])
    expect(columns.some((column) => column.isUnset)).toBe(false)
  })

  it('adds an (unset) bucket when a task has a status not in the project statuses', () => {
    const project = entry({
      path: '/Q2.md',
      title: 'Q2 Launch',
      isA: 'project',
      properties: { statuses: ['Backlog', 'Shipping'] },
    })
    const offSpec = entry({
      path: '/off.md',
      status: 'Mystery',
      relationships: { project: ['[[Q2 Launch]]'] },
    })
    const columns = deriveBoardColumns([offSpec], [project, offSpec], { property: 'status' })
    expect(columns.map((column) => column.key)).toEqual(['Backlog', 'Shipping', ''])
    const unset = columns.find((column) => column.isUnset)
    expect(unset?.entries.map((e) => e.path)).toEqual(['/off.md'])
  })

  it('groups by an arbitrary scalar property when no project binding', () => {
    const entries = [
      entry({ path: '/a.md', properties: { priority: 'P1' } }),
      entry({ path: '/b.md', properties: { priority: 'P2' } }),
      entry({ path: '/c.md', properties: {} }),
    ]
    const columns = deriveBoardColumns(entries, entries, { property: 'priority' })
    expect(columns.map((column) => column.key)).toEqual(['P1', 'P2', ''])
    expect(columns.find((column) => column.isUnset)?.entries.map((e) => e.path)).toEqual(['/c.md'])
  })

  it('honors the note. namespace prefix on the property', () => {
    const entries = [entry({ path: '/a.md', status: 'Backlog' })]
    const columns = deriveBoardColumns(entries, entries, { property: 'note.status' })
    expect(columns.map((column) => column.key)).toEqual(['Backlog'])
  })

  it('uses the provided unset label', () => {
    const entries = [entry({ path: '/a.md', status: null })]
    const columns = deriveBoardColumns(entries, entries, { property: 'status' }, '— none —')
    expect(columns[0].label).toBe('— none —')
  })
})
