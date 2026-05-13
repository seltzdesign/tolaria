import { invoke } from '@tauri-apps/api/core'
import { useCallback, useMemo } from 'react'
import { isTauri, mockInvoke } from '../mock-tauri'

/**
 * Hook returning thin wrappers around the [`create_task`](../../src-tauri/src/commands/tasks.rs)
 * and [`create_project`](../../src-tauri/src/commands/tasks.rs) Tauri commands.
 *
 * Listing tasks/projects is the existing entry scan; updating their properties
 * is the existing frontmatter mutation path (`runFrontmatterAndApply` + `save_note_content`).
 * This hook deliberately covers only the typed-creation surface.
 */

export interface CreateNoteResult {
  path: string
  warnings: string[]
}

export interface UseTasksApi {
  createTask: (
    folder: string,
    title: string,
    project?: string | null,
  ) => Promise<CreateNoteResult>
  createProject: (folder: string, title: string) => Promise<CreateNoteResult>
}

function invokeCommand<T>(command: string, args: Record<string, unknown>): Promise<T> {
  if (!isTauri()) return mockInvoke<T>(command, args)
  return invoke<T>(command, args)
}

export function useTasks(vaultPath: string): UseTasksApi {
  const createTask = useCallback<UseTasksApi['createTask']>(
    (folder, title, project) =>
      invokeCommand<CreateNoteResult>('create_task', {
        vaultPath,
        folder,
        title,
        project: project ?? null,
      }),
    [vaultPath],
  )

  const createProject = useCallback<UseTasksApi['createProject']>(
    (folder, title) =>
      invokeCommand<CreateNoteResult>('create_project', { vaultPath, folder, title }),
    [vaultPath],
  )

  return useMemo(() => ({ createTask, createProject }), [createTask, createProject])
}
