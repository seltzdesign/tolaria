import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const invoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}))

import { GitHubProjectsSettingsSection } from './GitHubProjectsSettingsSection'
import { createTranslator } from '../lib/i18n'

const t = createTranslator('en')

interface RenderArgs {
  enabled?: boolean
  syncIntervalMinutes?: number
  setEnabled?: (value: boolean) => void
  setSyncIntervalMinutes?: (value: number) => void
}

function renderSection(args: RenderArgs = {}) {
  return render(
    <GitHubProjectsSettingsSection
      t={t}
      enabled={args.enabled ?? true}
      setEnabled={args.setEnabled ?? (() => {})}
      syncIntervalMinutes={args.syncIntervalMinutes ?? 5}
      setSyncIntervalMinutes={args.setSyncIntervalMinutes ?? (() => {})}
    />,
  )
}

describe('GitHubProjectsSettingsSection', () => {
  beforeEach(() => {
    invoke.mockReset()
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'github_pat_present') return false
      return undefined
    })
  })

  it('renders the section heading and core controls', async () => {
    renderSection()
    expect(screen.getByText('GitHub Projects')).toBeInTheDocument()
    expect(screen.getByTestId('settings-github-projects-enabled')).toBeInTheDocument()
    expect(screen.getByTestId('settings-github-projects-pat-input')).toBeInTheDocument()
    expect(screen.getByTestId('settings-github-projects-save-pat')).toBeInTheDocument()
    expect(screen.getByTestId('settings-github-projects-test-connection')).toBeInTheDocument()
  })

  it('disables the PAT input, sync interval, save, and test buttons when sync is off', () => {
    renderSection({ enabled: false })
    expect(screen.getByTestId('settings-github-projects-pat-input')).toBeDisabled()
    expect(screen.getByTestId('settings-github-projects-save-pat')).toBeDisabled()
    expect(screen.getByTestId('settings-github-projects-sync-interval')).toBeDisabled()
    expect(screen.getByTestId('settings-github-projects-test-connection')).toBeDisabled()
  })

  it('disables Save while the PAT field is empty', () => {
    renderSection()
    expect(screen.getByTestId('settings-github-projects-save-pat')).toBeDisabled()
  })

  it('saves the PAT, clears the input, and refreshes presence', async () => {
    let presence = false
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'github_pat_present') return presence
      if (cmd === 'github_set_pat') {
        presence = true
        return undefined
      }
      return undefined
    })

    renderSection()
    const input = screen.getByTestId('settings-github-projects-pat-input') as HTMLInputElement
    fireEvent.change(input, { target: { value: 'ghp_secret' } })
    fireEvent.click(screen.getByTestId('settings-github-projects-save-pat'))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('github_set_pat', { pat: 'ghp_secret' })
    })
    await waitFor(() => {
      expect((screen.getByTestId('settings-github-projects-pat-input') as HTMLInputElement).value).toBe('')
    })
  })

  it('shows the connected GitHub login after a successful test', async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'github_pat_present') return true
      if (cmd === 'github_test_connection') return 'seltzdesign'
      return undefined
    })

    renderSection()
    await waitFor(() => {
      expect(screen.getByTestId('settings-github-projects-test-connection')).not.toBeDisabled()
    })
    fireEvent.click(screen.getByTestId('settings-github-projects-test-connection'))

    const status = await screen.findByTestId('settings-github-projects-status-success')
    expect(status).toHaveTextContent('@seltzdesign')
  })

  it('surfaces an error message when test connection fails', async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'github_pat_present') return true
      if (cmd === 'github_test_connection') throw 'Bad credentials'
      return undefined
    })

    renderSection()
    await waitFor(() => {
      expect(screen.getByTestId('settings-github-projects-test-connection')).not.toBeDisabled()
    })
    fireEvent.click(screen.getByTestId('settings-github-projects-test-connection'))

    const status = await screen.findByTestId('settings-github-projects-status-error')
    expect(status).toHaveTextContent('Bad credentials')
  })

  it('disables Test connection when no PAT is stored', () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'github_pat_present') return false
      return undefined
    })
    renderSection()
    expect(screen.getByTestId('settings-github-projects-test-connection')).toBeDisabled()
  })

  it('clears the stored PAT and refreshes presence', async () => {
    let presence = true
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'github_pat_present') return presence
      if (cmd === 'github_clear_pat') {
        presence = false
        return undefined
      }
      return undefined
    })

    renderSection()
    await waitFor(() => {
      expect(screen.getByTestId('settings-github-projects-clear-pat')).not.toBeDisabled()
    })
    fireEvent.click(screen.getByTestId('settings-github-projects-clear-pat'))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('github_clear_pat')
    })
    await waitFor(() => {
      expect(screen.getByTestId('settings-github-projects-clear-pat')).toBeDisabled()
    })
  })
})
