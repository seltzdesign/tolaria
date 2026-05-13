import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { VaultEntry, ViewFile } from '../../types'
import { TaskTable } from './TaskTable'

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

function tableView(overrides: Partial<ViewFile['definition']> = {}, fileOverrides: Partial<ViewFile> = {}): ViewFile {
  return {
    filename: 'table.yml',
    definition: {
      name: 'Table',
      icon: null,
      color: null,
      sort: null,
      filters: { all: [] },
      display: 'table',
      ...overrides,
    },
    ...fileOverrides,
  }
}

describe('TaskTable rendering', () => {
  it('renders the empty state when there are no filtered entries', () => {
    render(<TaskTable view={tableView()} filteredEntries={[]} onUpdateViewDefinition={vi.fn()} locale="en" />)
    expect(screen.getByText(/no items in this view/i)).toBeInTheDocument()
    expect(screen.queryByTestId('task-table')).not.toBeInTheDocument()
  })

  it('renders default columns when the view declares no columns', () => {
    const entries = [entry({ path: '/a.md', title: 'Alpha' })]
    render(<TaskTable view={tableView()} filteredEntries={entries} onUpdateViewDefinition={vi.fn()} locale="en" />)
    expect(screen.getByTestId('task-table-header-title')).toBeInTheDocument()
    expect(screen.getByTestId('task-table-header-status')).toBeInTheDocument()
    expect(screen.getByTestId('task-table-header-priority')).toBeInTheDocument()
    expect(screen.getByTestId('task-table-header-due')).toBeInTheDocument()
  })

  it('renders one row per filtered entry', () => {
    const entries = [
      entry({ path: '/a.md', title: 'Alpha' }),
      entry({ path: '/b.md', title: 'Beta' }),
    ]
    render(<TaskTable view={tableView()} filteredEntries={entries} onUpdateViewDefinition={vi.fn()} locale="en" />)
    expect(screen.getAllByTestId('task-table-row')).toHaveLength(2)
  })

  it('renders the columns declared by the view in order', () => {
    const entries = [entry()]
    render(
      <TaskTable
        view={tableView({ columns: ['title', 'priority', 'file.mtime'] })}
        filteredEntries={entries}
        onUpdateViewDefinition={vi.fn()}
        locale="en"
      />,
    )
    expect(screen.getByTestId('task-table-header-title')).toBeInTheDocument()
    expect(screen.getByTestId('task-table-header-priority')).toBeInTheDocument()
    expect(screen.getByTestId('task-table-header-file.mtime')).toBeInTheDocument()
    expect(screen.queryByTestId('task-table-header-status')).not.toBeInTheDocument()
  })

  it('calls onSelectNote when a row is clicked', () => {
    const onSelectNote = vi.fn()
    const entries = [entry({ path: '/a.md', title: 'Alpha' })]
    render(<TaskTable view={tableView()} filteredEntries={entries} onSelectNote={onSelectNote} locale="en" />)
    fireEvent.click(screen.getByTestId('task-table-row'))
    expect(onSelectNote).toHaveBeenCalledWith(expect.objectContaining({ path: '/a.md' }))
  })

  it('persists sort when a header is clicked', () => {
    const onUpdateViewDefinition = vi.fn()
    const entries = [entry({ path: '/a.md' })]
    render(
      <TaskTable
        view={tableView()}
        filteredEntries={entries}
        onUpdateViewDefinition={onUpdateViewDefinition}
        locale="en"
      />,
    )
    fireEvent.click(screen.getByTestId('task-table-header-priority'))
    expect(onUpdateViewDefinition).toHaveBeenCalledWith('table.yml', { sort: 'property:priority:asc' }, undefined)
  })

  it('flips sort direction when the same header is clicked twice', () => {
    const onUpdateViewDefinition = vi.fn()
    const entries = [entry({ path: '/a.md' })]
    render(
      <TaskTable
        view={tableView({ sort: 'title:asc' })}
        filteredEntries={entries}
        onUpdateViewDefinition={onUpdateViewDefinition}
        locale="en"
      />,
    )
    fireEvent.click(screen.getByTestId('task-table-header-title'))
    expect(onUpdateViewDefinition).toHaveBeenCalledWith('table.yml', { sort: 'title:desc' }, undefined)
  })

  it('passes the view rootPath through to onUpdateViewDefinition', () => {
    const onUpdateViewDefinition = vi.fn()
    const entries = [entry({ path: '/a.md' })]
    render(
      <TaskTable
        view={tableView({}, { rootPath: '/workspace' })}
        filteredEntries={entries}
        onUpdateViewDefinition={onUpdateViewDefinition}
        locale="en"
      />,
    )
    fireEvent.click(screen.getByTestId('task-table-header-status'))
    expect(onUpdateViewDefinition).toHaveBeenCalledWith('table.yml', { sort: 'status:asc' }, '/workspace')
  })
})
