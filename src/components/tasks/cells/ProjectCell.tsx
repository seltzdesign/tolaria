import { useState } from 'react'
import { Input } from '../../ui/input'

export interface ProjectCellProps {
  value: string | null
  onChange: (value: string | null) => void
  disabled?: boolean
  placeholder?: string
}

export function ProjectCell({
  value,
  onChange,
  disabled = false,
  placeholder = 'Project',
}: ProjectCellProps) {
  const [draft, setDraft] = useState(value ?? '')
  const [lastValue, setLastValue] = useState<string | null>(value)
  if (value !== lastValue) {
    setLastValue(value)
    setDraft(value ?? '')
  }

  const commit = () => {
    const trimmed = draft.trim()
    const next = trimmed.length > 0 ? trimmed : null
    if (next !== (value ?? null)) onChange(next)
  }

  return (
    <Input
      value={draft}
      placeholder={placeholder}
      disabled={disabled}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          event.preventDefault()
          commit()
        }
      }}
      data-testid="task-project-input"
      className="h-8 w-40"
    />
  )
}
