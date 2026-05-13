import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { VaultEntry, ViewFile } from '../../types'
import { TaskBoard } from './TaskBoard'
import { deriveBoardColumns, planBoardDrop } from '../../lib/tasks/boardColumns'

vi.mock('../../lib/telemetry', () => ({
  trackEvent: vi.fn(),
}))

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

function boardView(overrides: Partial<ViewFile['definition']> = {}): ViewFile {
  return {
    filename: 'board.yml',
    definition: {
      name: 'Board',
      icon: null,
      color: null,
      sort: null,
      filters: { all: [] },
      display: 'board',
      groupBy: { property: 'status' },
      ...overrides,
    },
  }
}

describe('TaskBoard rendering', () => {
  it('renders the empty state copy when there are no filtered entries', () => {
    render(
      <TaskBoard
        view={boardView()}
        filteredEntries={[]}
        allEntries={[]}
        onUpdateFrontmatter={vi.fn()}
        locale="en"
      />,
    )
    expect(screen.getByText(/no items in this view/i)).toBeInTheDocument()
    expect(screen.queryByTestId('board-column')).not.toBeInTheDocument()
  })

  it('renders one column per derived column with correct labels and counts', () => {
    const tasks = [
      entry({ path: '/a.md', status: 'Backlog' }),
      entry({ path: '/b.md', status: 'In progress' }),
      entry({ path: '/c.md', status: 'In progress' }),
    ]
    render(
      <TaskBoard
        view={boardView()}
        filteredEntries={tasks}
        allEntries={tasks}
        onUpdateFrontmatter={vi.fn()}
        locale="en"
      />,
    )
    const columns = screen.getAllByTestId('board-column')
    expect(columns).toHaveLength(2)
    const keys = columns.map((column) => column.getAttribute('data-column-key'))
    expect(keys).toEqual(['Backlog', 'In progress'])
  })
})

describe('planBoardDrop', () => {
  const tasks = [
    entry({ path: '/a.md', status: 'Backlog' }),
    entry({ path: '/b.md', status: 'In progress' }),
  ]
  const columns = deriveBoardColumns(tasks, tasks, { property: 'status' })

  it('returns the new status when a card is dropped on a different column', () => {
    const plan = planBoardDrop({
      filteredEntries: tasks,
      columns,
      writeField: 'status',
      activeId: '/a.md',
      overId: 'column:In progress',
    })
    expect(plan).toEqual({ path: '/a.md', field: 'status', from: 'Backlog', to: 'In progress' })
  })

  it('returns null when the card is dropped on its current column (no-op)', () => {
    const plan = planBoardDrop({
      filteredEntries: tasks,
      columns,
      writeField: 'status',
      activeId: '/a.md',
      overId: 'column:Backlog',
    })
    expect(plan).toBeNull()
  })

  it('returns null when there is no drop target', () => {
    const plan = planBoardDrop({
      filteredEntries: tasks,
      columns,
      writeField: 'status',
      activeId: '/a.md',
      overId: null,
    })
    expect(plan).toBeNull()
  })

  it('writes null when the card is dropped on the (unset) column', () => {
    const withUnset = [...tasks, entry({ path: '/c.md', status: null })]
    const cols = deriveBoardColumns(withUnset, withUnset, { property: 'status' })
    const plan = planBoardDrop({
      filteredEntries: withUnset,
      columns: cols,
      writeField: 'status',
      activeId: '/a.md',
      overId: 'column:',
    })
    expect(plan).toEqual({ path: '/a.md', field: 'status', from: 'Backlog', to: null })
  })
})
