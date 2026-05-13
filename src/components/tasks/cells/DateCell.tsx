import { useCallback, useState } from 'react'
import {
  formatDateOrDateTime,
  parseDateOrDateTime,
  toNaiveDate,
  type DateOrDateTime,
} from '../../../lib/tasks/dateOrDateTime'
import { Button } from '../../ui/button'
import { Calendar } from '../../ui/calendar'
import { Popover, PopoverContent, PopoverTrigger } from '../../ui/popover'

export interface DateCellProps {
  label: string
  value: DateOrDateTime | null
  onChange: (value: DateOrDateTime | null) => void
  disabled?: boolean
  clearLabel?: string
}

function dateToIsoDate(date: Date): string {
  const y = date.getFullYear()
  const m = String(date.getMonth() + 1).padStart(2, '0')
  const d = String(date.getDate()).padStart(2, '0')
  return `${y}-${m}-${d}`
}

function isoDateToDate(iso: string): Date | undefined {
  const parsed = parseDateOrDateTime(iso)
  if (!parsed) return undefined
  const [y, m, d] = toNaiveDate(parsed).split('-').map(Number)
  return new Date(y, m - 1, d)
}

function applyDate(previous: DateOrDateTime | null, newDate: string): DateOrDateTime {
  if (previous && previous.kind === 'datetime') {
    const tail = previous.iso.slice(10)
    return { kind: 'datetime', iso: `${newDate}${tail}` }
  }
  return { kind: 'date', date: newDate }
}

export function DateCell({
  label,
  value,
  onChange,
  disabled = false,
  clearLabel = 'Clear',
}: DateCellProps) {
  const [open, setOpen] = useState(false)
  const display = value ? formatDateOrDateTime(value) : null
  const selected = value ? isoDateToDate(formatDateOrDateTime(value)) : undefined

  const handleSelect = useCallback(
    (date: Date | undefined) => {
      if (!date) {
        onChange(null)
        setOpen(false)
        return
      }
      onChange(applyDate(value, dateToIsoDate(date)))
      setOpen(false)
    },
    [onChange, value],
  )

  const handleClear = useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      event.stopPropagation()
      onChange(null)
      setOpen(false)
    },
    [onChange],
  )

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          disabled={disabled}
          data-testid={`task-date-trigger-${label.toLowerCase()}`}
          className="justify-between font-normal"
        >
          <span className="text-muted-foreground mr-2">{label}</span>
          <span>{display ?? '—'}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-0" align="start">
        <Calendar mode="single" selected={selected} onSelect={handleSelect} />
        {value !== null
          ? (
            <div className="border-t p-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={handleClear}
                data-testid={`task-date-clear-${label.toLowerCase()}`}
              >
                {clearLabel}
              </Button>
            </div>
          )
          : null}
      </PopoverContent>
    </Popover>
  )
}
