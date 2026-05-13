import { fireEvent, render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { VaultEntry } from '../../types'
import { TaskHeader } from './TaskHeader'

vi.mock('../../lib/telemetry', () => ({
  trackEvent: vi.fn(),
}))

import { trackEvent } from '../../lib/telemetry'

function baseEntry(overrides: Partial<VaultEntry> = {}): VaultEntry {
  return {
    path: '/vault/task.md',
    filename: 'task.md',
    title: 'Task',
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

describe('TaskHeader', () => {
  beforeEach(() => {
    vi.mocked(trackEvent).mockReset()
  })

  it('renders priority, dates, labels and assignee cells', () => {
    render(<TaskHeader entry={baseEntry()} entries={[]} onUpdate={vi.fn()} />)
    expect(screen.getByTestId('task-status-trigger')).toBeInTheDocument()
    expect(screen.getByTestId('task-priority-trigger')).toBeInTheDocument()
    expect(screen.getByTestId('task-date-trigger-due')).toBeInTheDocument()
    expect(screen.getByTestId('task-date-trigger-start')).toBeInTheDocument()
    expect(screen.getByTestId('task-estimate-input')).toBeInTheDocument()
    expect(screen.getByTestId('task-project-input')).toBeInTheDocument()
  })

  it('emits an onUpdate for estimate input on blur', () => {
    const onUpdate = vi.fn()
    render(<TaskHeader entry={baseEntry()} entries={[]} onUpdate={onUpdate} />)
    const input = screen.getByTestId('task-estimate-input') as HTMLInputElement
    fireEvent.change(input, { target: { value: '5' } })
    fireEvent.blur(input)
    expect(onUpdate).toHaveBeenCalledWith('estimate', 5)
    expect(trackEvent).toHaveBeenCalledWith('task_property_edited', { property: 'estimate' })
  })

  it('emits a wikilink-shaped value when setting the project', () => {
    const onUpdate = vi.fn()
    render(<TaskHeader entry={baseEntry()} entries={[]} onUpdate={onUpdate} />)
    const input = screen.getByTestId('task-project-input') as HTMLInputElement
    fireEvent.change(input, { target: { value: 'Q2 Launch' } })
    fireEvent.blur(input)
    expect(onUpdate).toHaveBeenCalledWith('project', '[[Q2 Launch]]')
  })

  it('emits null when clearing the project', () => {
    const onUpdate = vi.fn()
    const entry = baseEntry({
      relationships: { project: ['[[Q2 Launch]]'] },
    })
    render(<TaskHeader entry={entry} entries={[]} onUpdate={onUpdate} />)
    const input = screen.getByTestId('task-project-input') as HTMLInputElement
    fireEvent.change(input, { target: { value: '   ' } })
    fireEvent.blur(input)
    expect(onUpdate).toHaveBeenCalledWith('project', null)
  })

  it('adds a chip and emits the wikilink-wrapped array for assignees', () => {
    const onUpdate = vi.fn()
    render(<TaskHeader entry={baseEntry()} entries={[]} onUpdate={onUpdate} />)
    const chipScope = screen.getByTestId('task-chips-assignees')
    const input = within(chipScope).getByTestId('task-chips-assignees-input') as HTMLInputElement
    fireEvent.change(input, { target: { value: 'Armin' } })
    fireEvent.keyDown(input, { key: 'Enter' })
    expect(onUpdate).toHaveBeenCalledWith('assignee', ['[[Armin]]'])
  })

  it('emits null when removing the last label', () => {
    const onUpdate = vi.fn()
    const entry = baseEntry({ properties: { labels: ['bug'] } })
    render(<TaskHeader entry={entry} entries={[]} onUpdate={onUpdate} />)
    const remove = screen.getByTestId('task-chip-remove-0')
    fireEvent.click(remove)
    expect(onUpdate).toHaveBeenCalledWith('labels', null)
  })

  it('uses project statuses when the entry has a known project', () => {
    const onUpdate = vi.fn()
    const projectEntry = baseEntry({
      path: '/vault/Q2 Launch.md',
      title: 'Q2 Launch',
      isA: 'project',
      properties: { statuses: ['Backlog', 'Shipping'] },
    })
    const taskEntry = baseEntry({
      relationships: { project: ['[[Q2 Launch]]'] },
    })
    render(<TaskHeader entry={taskEntry} entries={[projectEntry]} onUpdate={onUpdate} />)
    const trigger = screen.getByTestId('task-status-trigger')
    expect(trigger).toBeInTheDocument()
  })
})
