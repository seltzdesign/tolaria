import { useState } from 'react'
import { Input } from '../../ui/input'

export interface EstimateCellProps {
  value: number | null
  onChange: (value: number | null) => void
  disabled?: boolean
  placeholder?: string
}

function parseEstimate(raw: string): number | null {
  const trimmed = raw.trim()
  if (!trimmed) return null
  const parsed = Number(trimmed)
  if (!Number.isFinite(parsed) || parsed < 0) return null
  return parsed
}

function draftFromValue(value: number | null): string {
  return value === null ? '' : String(value)
}

export function EstimateCell({
  value,
  onChange,
  disabled = false,
  placeholder = 'Estimate',
}: EstimateCellProps) {
  const [draft, setDraft] = useState(() => draftFromValue(value))
  const [lastValue, setLastValue] = useState<number | null>(value)
  if (value !== lastValue) {
    setLastValue(value)
    setDraft(draftFromValue(value))
  }

  const commit = () => {
    const next = parseEstimate(draft)
    if (next !== value) onChange(next)
  }

  return (
    <Input
      type="number"
      min={0}
      step="0.5"
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
      data-testid="task-estimate-input"
      className="h-8 w-24"
    />
  )
}
