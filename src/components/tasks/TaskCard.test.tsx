import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { VaultEntry } from '../../types'
import { TaskCard } from './TaskCard'

function entry(overrides: Partial<VaultEntry> = {}): VaultEntry {
  return {
    path: '/vault/task.md',
    filename: 'task.md',
    title: 'Build the board',
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

describe('TaskCard', () => {
  it('renders the title and falls back to the filename when title is empty', () => {
    render(<TaskCard entry={entry({ title: '' })} locale="en" />)
    expect(screen.getByText('task.md')).toBeInTheDocument()
  })

  it('renders the priority chip and due badge for task entries', () => {
    const taskEntry = entry({
      properties: { priority: 'P1', due: '2099-12-31' },
    })
    render(<TaskCard entry={taskEntry} locale="en" />)
    expect(screen.getByText('P1')).toBeInTheDocument()
    expect(screen.getByText(/dec/i)).toBeInTheDocument()
  })

  it('renders an assignee badge when there are assignees', () => {
    const taskEntry = entry({
      relationships: { assignee: ['[[Armin]]', '[[Co-pilot]]'] },
    })
    render(<TaskCard entry={taskEntry} locale="en" />)
    expect(screen.getByText(/2 assignee/i)).toBeInTheDocument()
  })

  it('calls onSelect when clicked', () => {
    const onSelect = vi.fn()
    render(<TaskCard entry={entry()} onSelect={onSelect} locale="en" />)
    fireEvent.click(screen.getByTestId('task-card'))
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ path: '/vault/task.md' }))
  })

  it('renders title only for non-task entries (no priority/due chips)', () => {
    const nonTask = entry({
      isA: 'note',
      properties: { priority: 'P0', due: '2099-12-31' },
    })
    render(<TaskCard entry={nonTask} locale="en" />)
    expect(screen.getByText('Build the board')).toBeInTheDocument()
    expect(screen.queryByText('P0')).not.toBeInTheDocument()
  })
})
