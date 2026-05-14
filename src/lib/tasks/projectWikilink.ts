import type { VaultEntry } from '../../types'

function filenameStem(entry: VaultEntry): string {
  return entry.filename.replace(/\.md$/i, '')
}

/** Build the `[[…]]` wikilink to write into a task's `project` frontmatter when the
 * dropdown selects a project entry by path. Returns `null` when the selection is
 * cleared, or when the path no longer matches a known entry (treat as cleared). */
export function buildProjectWikilinkValue(
  nextPath: string | null,
  entries: readonly VaultEntry[],
): string | null {
  if (!nextPath) return null
  const target = entries.find((candidate) => candidate.path === nextPath)
  return target ? `[[${filenameStem(target)}]]` : null
}
