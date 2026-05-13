import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { VaultEntry, ViewFile } from '../../types'
import { TaskTimeline } from './TaskTimeline'

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

function timelineView(overrides: Partial<ViewFile['definition']> = {}): ViewFile {
  return {
    filename: 'timeline.yml',
    definition: {
      name: 'Timeline',
      icon: null,
      color: null,
      sort: null,
      filters: { all: [] },
      display: 'timeline',
      groupBy: { property: 'status' },
      ...overrides,
    },
  }
}

describe('TaskTimeline rendering', () => {
  it('renders the empty state when there are no filtered entries', () => {
    render(
      <TaskTimeline
        view={timelineView()}
        filteredEntries={[]}
        onUpdateFrontmatter={vi.fn()}
        locale="en"
      />,
    )
    expect(screen.getByText(/no items in this view/i)).toBeInTheDocument()
    expect(screen.queryByTestId('task-timeline')).not.toBeInTheDocument()
  })

  it('renders one bar per entry with start and due dates', () => {
    const entries = [
      entry({ path: '/a.md', status: 'Open', properties: { start: '2026-05-10', due: '2026-05-15' } }),
      entry({ path: '/b.md', status: 'Open', properties: { start: '2026-05-12', due: '2026-05-18' } }),
    ]
    render(
      <TaskTimeline
        view={timelineView()}
        filteredEntries={entries}
        onUpdateFrontmatter={vi.fn()}
        locale="en"
      />,
    )
    expect(screen.getByTestId('timeline-bar-/a.md')).toBeInTheDocument()
    expect(screen.getByTestId('timeline-bar-/b.md')).toBeInTheDocument()
  })

  it('omits date-less entries and shows the undated footer', () => {
    const entries = [
      entry({ path: '/a.md', status: 'Open', properties: { start: '2026-05-10', due: '2026-05-15' } }),
      entry({ path: '/b.md', status: 'Open' }),
    ]
    render(
      <TaskTimeline
        view={timelineView()}
        filteredEntries={entries}
        onUpdateFrontmatter={vi.fn()}
        locale="en"
      />,
    )
    expect(screen.getByTestId('timeline-bar-/a.md')).toBeInTheDocument()
    expect(screen.queryByTestId('timeline-bar-/b.md')).not.toBeInTheDocument()
    expect(screen.getByTestId('timeline-undated-footer')).toHaveTextContent(/1 task without dates/i)
  })

  it('clicking a bar without movement calls onSelectNote', () => {
    const onSelectNote = vi.fn()
    const entries = [entry({ path: '/a.md', status: 'Open', properties: { start: '2026-05-10', due: '2026-05-15' } })]
    render(
      <TaskTimeline
        view={timelineView()}
        filteredEntries={entries}
        onSelectNote={onSelectNote}
        onUpdateFrontmatter={vi.fn()}
        locale="en"
      />,
    )
    const bar = screen.getByTestId('timeline-bar-/a.md')
    fireEvent.pointerDown(bar, { clientX: 100, pointerId: 1 })
    fireEvent.pointerUp(bar, { clientX: 100, pointerId: 1 })
    expect(onSelectNote).toHaveBeenCalledWith(expect.objectContaining({ path: '/a.md' }))
  })

  it('dragging the bar body shifts both start and due frontmatter writes', () => {
    const onUpdateFrontmatter = vi.fn()
    const entries = [entry({ path: '/a.md', status: 'Open', properties: { start: '2026-05-10', due: '2026-05-15' } })]
    render(
      <TaskTimeline
        view={timelineView()}
        filteredEntries={entries}
        onSelectNote={vi.fn()}
        onUpdateFrontmatter={onUpdateFrontmatter}
        locale="en"
      />,
    )
    const bar = screen.getByTestId('timeline-bar-/a.md')
    fireEvent.pointerDown(bar, { clientX: 0, pointerId: 1 })
    fireEvent.pointerMove(bar, { clientX: 80, pointerId: 1 })
    fireEvent.pointerUp(bar, { clientX: 80, pointerId: 1 })
    expect(onUpdateFrontmatter).toHaveBeenCalledWith('/a.md', 'start', '2026-05-12')
    expect(onUpdateFrontmatter).toHaveBeenCalledWith('/a.md', 'due', '2026-05-17')
  })

  it('dragging the right edge writes only due', () => {
    const onUpdateFrontmatter = vi.fn()
    const entries = [entry({ path: '/a.md', status: 'Open', properties: { start: '2026-05-10', due: '2026-05-15' } })]
    render(
      <TaskTimeline
        view={timelineView()}
        filteredEntries={entries}
        onSelectNote={vi.fn()}
        onUpdateFrontmatter={onUpdateFrontmatter}
        locale="en"
      />,
    )
    const handle = screen.getByTestId('timeline-bar-handle-end-/a.md')
    fireEvent.pointerDown(handle, { clientX: 0, pointerId: 1 })
    fireEvent.pointerMove(handle, { clientX: 80, pointerId: 1 })
    fireEvent.pointerUp(handle, { clientX: 80, pointerId: 1 })
    expect(onUpdateFrontmatter).toHaveBeenCalledWith('/a.md', 'due', '2026-05-17')
    expect(onUpdateFrontmatter).not.toHaveBeenCalledWith('/a.md', 'start', expect.anything())
  })

  it('dragging the left edge writes only start', () => {
    const onUpdateFrontmatter = vi.fn()
    const entries = [entry({ path: '/a.md', status: 'Open', properties: { start: '2026-05-10', due: '2026-05-15' } })]
    render(
      <TaskTimeline
        view={timelineView()}
        filteredEntries={entries}
        onSelectNote={vi.fn()}
        onUpdateFrontmatter={onUpdateFrontmatter}
        locale="en"
      />,
    )
    const handle = screen.getByTestId('timeline-bar-handle-start-/a.md')
    fireEvent.pointerDown(handle, { clientX: 0, pointerId: 1 })
    fireEvent.pointerMove(handle, { clientX: -40, pointerId: 1 })
    fireEvent.pointerUp(handle, { clientX: -40, pointerId: 1 })
    expect(onUpdateFrontmatter).toHaveBeenCalledWith('/a.md', 'start', '2026-05-09')
    expect(onUpdateFrontmatter).not.toHaveBeenCalledWith('/a.md', 'due', expect.anything())
  })
})
