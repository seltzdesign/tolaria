import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { VaultEntry } from '../../types'
import { TaskEditor } from './TaskEditor'

vi.mock('../../lib/telemetry', () => ({
  trackEvent: vi.fn(),
}))

function baseEntry(overrides: Partial<VaultEntry> = {}): VaultEntry {
  return {
    path: '/vault/x.md',
    filename: 'x.md',
    title: 'X',
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

describe('TaskEditor', () => {
  it('renders the task header above the body when entry is a task', () => {
    render(
      <TaskEditor entry={baseEntry()} entries={[]} onUpdate={vi.fn()}>
        <div data-testid="body">body</div>
      </TaskEditor>,
    )
    expect(screen.getByTestId('task-header')).toBeInTheDocument()
    expect(screen.getByTestId('body')).toBeInTheDocument()
  })

  it('renders only the body when entry is not a task', () => {
    render(
      <TaskEditor entry={baseEntry({ isA: 'Note' })} entries={[]} onUpdate={vi.fn()}>
        <div data-testid="body">body</div>
      </TaskEditor>,
    )
    expect(screen.queryByTestId('task-header')).not.toBeInTheDocument()
    expect(screen.getByTestId('body')).toBeInTheDocument()
  })
})
