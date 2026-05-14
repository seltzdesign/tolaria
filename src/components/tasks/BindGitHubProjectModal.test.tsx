import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const invoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}))

import { BindGitHubProjectModal } from './BindGitHubProjectModal'

const resolveResponse = {
  project: {
    id: 'PVT_abc',
    number: 7,
    title: 'Q2 Launch',
    url: 'https://github.com/users/x/projects/7',
    closed: false,
  },
  fields: [
    { id: 'F_title', name: 'Title', data_type: 'TITLE', options: [] },
    { id: 'F_status', name: 'Status', data_type: 'SINGLE_SELECT', options: [{ id: 'o1', name: 'Backlog' }] },
    { id: 'F_priority', name: 'Priority', data_type: 'SINGLE_SELECT', options: [] },
    { id: 'F_due', name: 'End date', data_type: 'DATE', options: [] },
  ],
}

function renderModal(overrides: Partial<React.ComponentProps<typeof BindGitHubProjectModal>> = {}) {
  return render(
    <BindGitHubProjectModal
      open
      notePath="/vault/q2-launch.md"
      initialUrl={null}
      alreadyBound={false}
      onClose={vi.fn()}
      {...overrides}
    />,
  )
}

describe('BindGitHubProjectModal', () => {
  beforeEach(() => {
    invoke.mockReset()
  })

  it('renders the URL input + Resolve button', () => {
    renderModal()
    expect(screen.getByTestId('github-bind-url-input')).toBeInTheDocument()
    expect(screen.getByTestId('github-bind-resolve')).toBeInTheDocument()
  })

  it('disables Resolve until a URL is entered', () => {
    renderModal()
    expect(screen.getByTestId('github-bind-resolve')).toBeDisabled()
    fireEvent.change(screen.getByTestId('github-bind-url-input'), {
      target: { value: 'https://github.com/users/x/projects/7' },
    })
    expect(screen.getByTestId('github-bind-resolve')).not.toBeDisabled()
  })

  it('surfaces a resolution error returned from the backend', async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'github_resolve_project_url') throw 'No such project'
      return undefined
    })
    renderModal()
    fireEvent.change(screen.getByTestId('github-bind-url-input'), {
      target: { value: 'https://github.com/users/x/projects/7' },
    })
    fireEvent.click(screen.getByTestId('github-bind-resolve'))
    const error = await screen.findByTestId('github-bind-error')
    expect(error).toHaveTextContent('No such project')
  })

  it('shows the resolved project summary and field mapping rows on success', async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'github_resolve_project_url') return resolveResponse
      return undefined
    })
    renderModal()
    fireEvent.change(screen.getByTestId('github-bind-url-input'), {
      target: { value: 'https://github.com/users/x/projects/7' },
    })
    fireEvent.click(screen.getByTestId('github-bind-resolve'))
    const summary = await screen.findByTestId('github-bind-resolved-summary')
    expect(summary).toHaveTextContent('Q2 Launch')
    expect(summary).toHaveTextContent('#7')
    // Field rows exist for each local field key
    expect(screen.getByTestId('github-bind-field-status')).toBeInTheDocument()
    expect(screen.getByTestId('github-bind-field-priority')).toBeInTheDocument()
    expect(screen.getByTestId('github-bind-field-due')).toBeInTheDocument()
  })

  it('persists the binding on save with the project node id', async () => {
    const onBound = vi.fn()
    const onClose = vi.fn()
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'github_resolve_project_url') return resolveResponse
      if (cmd === 'github_bind_project') return undefined
      return undefined
    })
    renderModal({ onBound, onClose })
    fireEvent.change(screen.getByTestId('github-bind-url-input'), {
      target: { value: 'https://github.com/users/x/projects/7' },
    })
    fireEvent.click(screen.getByTestId('github-bind-resolve'))
    await screen.findByTestId('github-bind-resolved-summary')
    fireEvent.click(screen.getByTestId('github-bind-save'))
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'github_bind_project',
        expect.objectContaining({
          notePath: '/vault/q2-launch.md',
          bindingInput: expect.objectContaining({
            project_node_id: 'PVT_abc',
            project_url: 'https://github.com/users/x/projects/7',
            status_field: 'Status',
          }),
        }),
      )
    })
    await waitFor(() => expect(onBound).toHaveBeenCalled())
    await waitFor(() => expect(onClose).toHaveBeenCalled())
  })

  it('hides the Unbind action when the note is not yet bound', () => {
    renderModal({ alreadyBound: false })
    expect(screen.queryByTestId('github-bind-unbind')).not.toBeInTheDocument()
  })

  it('shows the Unbind action when the note is already bound and clears the binding', async () => {
    const onUnbound = vi.fn()
    const onClose = vi.fn()
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'github_unbind_project') return undefined
      return undefined
    })
    renderModal({
      alreadyBound: true,
      initialUrl: 'https://github.com/users/x/projects/7',
      onUnbound,
      onClose,
    })
    fireEvent.click(screen.getByTestId('github-bind-unbind'))
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('github_unbind_project', { notePath: '/vault/q2-launch.md' })
    })
    await waitFor(() => expect(onUnbound).toHaveBeenCalled())
    await waitFor(() => expect(onClose).toHaveBeenCalled())
  })
})
