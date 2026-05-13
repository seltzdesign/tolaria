import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../../ui/select'

const NONE_VALUE = '__none__'
const PRIORITY_OPTIONS = ['P0', 'P1', 'P2', 'P3'] as const

export interface PriorityCellProps {
  value: string | null
  onChange: (value: string | null) => void
  disabled?: boolean
}

export function PriorityCell({ value, onChange, disabled = false }: PriorityCellProps) {
  const selectValue = value ?? NONE_VALUE
  return (
    <Select
      value={selectValue}
      onValueChange={(next) => onChange(next === NONE_VALUE ? null : next)}
      disabled={disabled}
    >
      <SelectTrigger size="sm" data-testid="task-priority-trigger">
        <SelectValue placeholder="Priority" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={NONE_VALUE}>—</SelectItem>
        {PRIORITY_OPTIONS.map((option) => (
          <SelectItem key={option} value={option}>
            {option}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
