import { useState } from 'react'
import { Input } from '../../ui/input'

export interface CompletionCellProps {
  value: number | null
  onChange: (value: number | null) => void
  disabled?: boolean
  placeholder?: string
}

function clampPercent(value: number): number {
  if (value < 0) return 0
  if (value > 100) return 100
  return Math.round(value)
}

function parseCompletion(raw: string): number | null {
  const trimmed = raw.replace('%', '').trim()
  if (!trimmed) return null
  const parsed = Number(trimmed)
  if (!Number.isFinite(parsed)) return null
  return clampPercent(parsed)
}

function draftFromValue(value: number | null): string {
  return value === null ? '' : String(value)
}

export function CompletionCell({
  value,
  onChange,
  disabled = false,
  placeholder = '0',
}: CompletionCellProps) {
  const [draft, setDraft] = useState(() => draftFromValue(value))
  const [lastValue, setLastValue] = useState<number | null>(value)
  if (value !== lastValue) {
    setLastValue(value)
    setDraft(draftFromValue(value))
  }

  const commit = () => {
    const next = parseCompletion(draft)
    if (next !== value) onChange(next)
  }

  return (
    <div className="relative">
      <Input
        type="number"
        min={0}
        max={100}
        step={5}
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
        data-testid="task-completion-input"
        className="h-8 w-20 pr-6"
      />
      <span className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-xs text-muted-foreground">
        %
      </span>
    </div>
  )
}
