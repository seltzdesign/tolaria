import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { VaultEntry } from '../../types'
import { getColumnLabel, getColumnSortOption, renderColumnCell } from './columnCell'

function entry(overrides: Partial<VaultEntry> = {}): VaultEntry {
  return {
    path: '/vault/task.md',
    filename: 'task.md',
    title: 'Build the table',
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

describe('getColumnLabel', () => {
  it('returns localized labels for known fields', () => {
    expect(getColumnLabel('title', 'en')).toBe('Title')
    expect(getColumnLabel('priority', 'en')).toBe('Priority')
    expect(getColumnLabel('file.mtime', 'en')).toBe('Modified')
  })

  it('falls back to the bare name when no localization key matches', () => {
    expect(getColumnLabel('custom_key', 'en')).toBe('custom_key')
  })

  it('strips the note. prefix before resolving', () => {
    expect(getColumnLabel('note.status', 'en')).toBe('Status')
  })
})

describe('getColumnSortOption', () => {
  it('maps known sortable fields', () => {
    expect(getColumnSortOption('title')).toBe('title')
    expect(getColumnSortOption('status')).toBe('status')
    expect(getColumnSortOption('file.mtime')).toBe('modified')
    expect(getColumnSortOption('file.ctime')).toBe('created')
    expect(getColumnSortOption('priority')).toBe('property:priority')
  })

  it('returns null for unsortable file.X derivations', () => {
    expect(getColumnSortOption('file.path')).toBeNull()
    expect(getColumnSortOption('file.size')).toBeNull()
  })
})

describe('renderColumnCell', () => {
  it('renders the entry title for the title column', () => {
    render(<table><tbody><tr><td>{renderColumnCell(entry(), 'title', 'en')}</td></tr></tbody></table>)
    expect(screen.getByText('Build the table')).toBeInTheDocument()
  })

  it('renders the priority badge with the right value', () => {
    const taskEntry = entry({ properties: { priority: 'P0' } })
    render(<table><tbody><tr><td>{renderColumnCell(taskEntry, 'priority', 'en')}</td></tr></tbody></table>)
    expect(screen.getByText('P0')).toBeInTheDocument()
  })

  it('renders a date badge for the due column', () => {
    const taskEntry = entry({ properties: { due: '2099-12-31' } })
    render(<table><tbody><tr><td>{renderColumnCell(taskEntry, 'due', 'en')}</td></tr></tbody></table>)
    expect(screen.getByText(/dec/i)).toBeInTheDocument()
  })

  it('renders an em-dash placeholder for missing values', () => {
    render(<table><tbody><tr><td>{renderColumnCell(entry(), 'priority', 'en')}</td></tr></tbody></table>)
    expect(screen.getByText('—')).toBeInTheDocument()
  })

  it('renders assignees as a comma-joined list of wikilink targets', () => {
    const taskEntry = entry({ relationships: { assignee: ['[[Armin]]', '[[Co-pilot]]'] } })
    render(<table><tbody><tr><td>{renderColumnCell(taskEntry, 'assignees', 'en')}</td></tr></tbody></table>)
    expect(screen.getByText('Armin, Co-pilot')).toBeInTheDocument()
  })

  it('renders labels from the properties array', () => {
    const taskEntry = entry({ properties: { labels: ['bug', 'frontend'] } })
    render(<table><tbody><tr><td>{renderColumnCell(taskEntry, 'labels', 'en')}</td></tr></tbody></table>)
    expect(screen.getByText('bug, frontend')).toBeInTheDocument()
  })

  it('renders file.path from the entry path', () => {
    const taskEntry = entry({ path: '/vault/Projects/foo.md' })
    render(<table><tbody><tr><td>{renderColumnCell(taskEntry, 'file.path', 'en')}</td></tr></tbody></table>)
    expect(screen.getByText('/vault/Projects/foo.md')).toBeInTheDocument()
  })

  it('renders a fallback property for unknown bare names', () => {
    const taskEntry = entry({ properties: { my_custom: 'whatever' } })
    render(<table><tbody><tr><td>{renderColumnCell(taskEntry, 'my_custom', 'en')}</td></tr></tbody></table>)
    expect(screen.getByText('whatever')).toBeInTheDocument()
  })
})
