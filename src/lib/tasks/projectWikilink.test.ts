import { describe, expect, it } from 'vitest'
import type { VaultEntry } from '../../types'
import { buildProjectWikilinkValue } from './projectWikilink'

function baseEntry(overrides: Partial<VaultEntry> = {}): VaultEntry {
  return {
    path: '/vault/note.md',
    filename: 'note.md',
    title: 'Note',
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

describe('buildProjectWikilinkValue', () => {
  const project = baseEntry({
    path: '/vault/q2-launch.md',
    filename: 'q2-launch.md',
    title: 'Q2 Launch',
    isA: 'project',
  })

  it('returns null when nextPath is null', () => {
    expect(buildProjectWikilinkValue(null, [project])).toBeNull()
  })

  it('builds a wikilink from the project filename stem when nextPath matches an entry', () => {
    expect(buildProjectWikilinkValue('/vault/q2-launch.md', [project])).toBe('[[q2-launch]]')
  })

  it('returns null when nextPath does not match any entry (treat as cleared)', () => {
    expect(buildProjectWikilinkValue('/vault/missing.md', [project])).toBeNull()
  })
})
