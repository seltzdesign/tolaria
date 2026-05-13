import { invoke } from '@tauri-apps/api/core'
import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useTasks } from './useTasks'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('../mock-tauri', () => ({
  isTauri: () => true,
  mockInvoke: vi.fn(),
}))

describe('useTasks', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset()
  })

  it('forwards createTask args to the Tauri command with vaultPath', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ path: '/vault/launch.md', warnings: [] })
    const { result } = renderHook(() => useTasks('/vault'))

    await act(async () => {
      await result.current.createTask('Projects', 'Launch', 'Q2')
    })

    expect(invoke).toHaveBeenCalledWith('create_task', {
      vaultPath: '/vault',
      folder: 'Projects',
      title: 'Launch',
      project: 'Q2',
    })
  })

  it('createTask sends null for project when omitted', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ path: '/vault/x.md', warnings: [] })
    const { result } = renderHook(() => useTasks('/vault'))

    await act(async () => {
      await result.current.createTask('', 'Untitled task')
    })

    expect(invoke).toHaveBeenCalledWith('create_task', {
      vaultPath: '/vault',
      folder: '',
      title: 'Untitled task',
      project: null,
    })
  })

  it('forwards createProject args to the Tauri command', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ path: '/vault/q2.md', warnings: [] })
    const { result } = renderHook(() => useTasks('/vault'))

    await act(async () => {
      await result.current.createProject('Projects', 'Q2')
    })

    expect(invoke).toHaveBeenCalledWith('create_project', {
      vaultPath: '/vault',
      folder: 'Projects',
      title: 'Q2',
    })
  })

  it('returns the CreateNoteResult shape from the command', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      path: '/vault/seeded.md',
      warnings: ['seeded task.md'],
    })
    const { result } = renderHook(() => useTasks('/vault'))

    let response
    await act(async () => {
      response = await result.current.createTask('', 'Seeded')
    })

    expect(response).toEqual({ path: '/vault/seeded.md', warnings: ['seeded task.md'] })
  })
})
