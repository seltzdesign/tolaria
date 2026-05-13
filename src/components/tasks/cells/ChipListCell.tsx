import { useState } from 'react'
import { Badge } from '../../ui/badge'
import { Input } from '../../ui/input'

export interface ChipListCellProps {
  label: string
  values: string[]
  placeholder?: string
  onChange: (next: string[]) => void
  disabled?: boolean
  testId?: string
}

export function ChipListCell({
  label,
  values,
  placeholder,
  onChange,
  disabled = false,
  testId,
}: ChipListCellProps) {
  const [draft, setDraft] = useState('')

  const addDraft = () => {
    const trimmed = draft.trim()
    if (!trimmed) return
    if (values.includes(trimmed)) {
      setDraft('')
      return
    }
    onChange([...values, trimmed])
    setDraft('')
  }

  const removeAt = (index: number) => {
    onChange(values.filter((_, i) => i !== index))
  }

  return (
    <div className="flex flex-wrap items-center gap-1" data-testid={testId ?? `task-chips-${label.toLowerCase()}`}>
      <span className="text-muted-foreground text-xs mr-1">{label}</span>
      {values.map((entry, index) => (
        <Badge
          key={`${entry}-${index}`}
          variant="secondary"
          className="flex items-center gap-1"
        >
          <span>{entry}</span>
          {!disabled
            ? (
              <button
                type="button"
                onClick={() => removeAt(index)}
                aria-label={`Remove ${entry}`}
                className="ml-1 text-muted-foreground hover:text-foreground"
                data-testid={`task-chip-remove-${index}`}
              >
                ×
              </button>
            )
            : null}
        </Badge>
      ))}
      <Input
        value={draft}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault()
            addDraft()
          } else if (event.key === 'Backspace' && draft === '' && values.length > 0) {
            event.preventDefault()
            removeAt(values.length - 1)
          }
        }}
        onBlur={addDraft}
        data-testid={`${testId ?? `task-chips-${label.toLowerCase()}`}-input`}
        className="h-7 w-32"
      />
    </div>
  )
}
