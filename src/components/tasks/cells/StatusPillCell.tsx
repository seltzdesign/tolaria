import { useMemo } from 'react'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../../ui/select'

const NONE_VALUE = '__none__'
const DEFAULT_OPTIONS = ['Open', 'In progress', 'Done'] as const

export interface StatusPillCellProps {
  value: string | null
  options?: readonly string[]
  onChange: (value: string | null) => void
  disabled?: boolean
  placeholder?: string
}

function dedupeOptions(options: readonly string[], current: string | null): string[] {
  const seen = new Set<string>()
  const result: string[] = []
  for (const opt of options) {
    if (!opt) continue
    if (seen.has(opt)) continue
    seen.add(opt)
    result.push(opt)
  }
  if (current && !seen.has(current)) result.push(current)
  return result
}

export function StatusPillCell({
  value,
  options,
  onChange,
  disabled = false,
  placeholder = 'Status',
}: StatusPillCellProps) {
  const resolvedOptions = useMemo(
    () => dedupeOptions(options && options.length > 0 ? options : DEFAULT_OPTIONS, value),
    [options, value],
  )
  const selectValue = value ?? NONE_VALUE
  return (
    <Select
      value={selectValue}
      onValueChange={(next) => onChange(next === NONE_VALUE ? null : next)}
      disabled={disabled}
    >
      <SelectTrigger size="sm" data-testid="task-status-trigger">
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={NONE_VALUE}>—</SelectItem>
        {resolvedOptions.map((option) => (
          <SelectItem key={option} value={option}>
            {option}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
