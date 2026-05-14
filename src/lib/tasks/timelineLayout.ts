import type { VaultEntry, ViewGroupBy } from '../../types'
import { parseDateOrDateTime, type DateOrDateTime } from './dateOrDateTime'
import { asTask } from './taskView'
import { normalizeBoardField } from './boardColumns'

const MS_PER_DAY = 86_400_000
const MIN_DAYS_BEFORE_TODAY = 365
const MIN_DAYS_AFTER_TODAY = 365
const PADDING_DAYS = 3

export interface DateRange {
  startMs: number
  endMs: number
  days: number
}

export interface BarLayout {
  entry: VaultEntry
  startMs: number
  endMs: number
  xPx: number
  widthPx: number
}

export interface LaneGroup {
  key: string
  label: string
  isUnset: boolean
  bars: BarLayout[]
}

export interface TimelineLayout {
  range: DateRange
  lanes: LaneGroup[]
  undatedCount: number
}

function dateOrDateTimeToMs(value: DateOrDateTime): number {
  if (value.kind === 'date') {
    const [year, month, day] = value.date.split('-').map(Number)
    return Date.UTC(year, month - 1, day)
  }
  return new Date(value.iso).getTime()
}

function readDateProperty(entry: VaultEntry, key: string): number | null {
  const raw = entry.properties[key]
  if (typeof raw !== 'string' || !raw) return null
  const parsed = parseDateOrDateTime(raw)
  return parsed ? dateOrDateTimeToMs(parsed) : null
}

function entryDateBounds(entry: VaultEntry): { start: number | null; due: number | null } {
  return {
    start: readDateProperty(entry, 'start'),
    due: readDateProperty(entry, 'due'),
  }
}

function startOfDayUtc(ms: number): number {
  return Math.floor(ms / MS_PER_DAY) * MS_PER_DAY
}

export function dateRangeFor(entries: VaultEntry[], today: Date): DateRange {
  const todayUtc = Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate())
  let min = todayUtc - MIN_DAYS_BEFORE_TODAY * MS_PER_DAY
  let max = todayUtc + MIN_DAYS_AFTER_TODAY * MS_PER_DAY
  for (const entry of entries) {
    const { start, due } = entryDateBounds(entry)
    for (const ms of [start, due]) {
      if (ms === null) continue
      const day = startOfDayUtc(ms)
      if (day - PADDING_DAYS * MS_PER_DAY < min) min = day - PADDING_DAYS * MS_PER_DAY
      if (day + PADDING_DAYS * MS_PER_DAY > max) max = day + PADDING_DAYS * MS_PER_DAY
    }
  }
  return { startMs: min, endMs: max, days: Math.round((max - min) / MS_PER_DAY) + 1 }
}

function readLaneValues(entry: VaultEntry, field: string): string[] {
  if (field === 'status') return entry.status ? [entry.status] : []
  const task = asTask(entry)
  if (task) {
    if (field === 'assignee' || field === 'assignees') {
      return task.assignees.length > 0 ? [task.assignees[0]] : []
    }
    if (field === 'project') return task.project ? [task.project] : []
    if (field === 'blocked_by' || field === 'blockedBy') {
      return task.blockedBy.length > 0 ? [task.blockedBy[0]] : []
    }
  }
  const value = entry.properties[field]
  if (typeof value === 'string' && value) return [value]
  if (typeof value === 'number' || typeof value === 'boolean') return [String(value)]
  return []
}

function clampBarBounds(start: number | null, due: number | null): { start: number; end: number } | null {
  if (start === null && due === null) return null
  if (start === null) return { start: due!, end: due! + MS_PER_DAY }
  if (due === null) return { start, end: start + MS_PER_DAY }
  const lo = Math.min(start, due)
  const hi = Math.max(start, due)
  return lo === hi ? { start: lo, end: lo + MS_PER_DAY } : { start: lo, end: hi }
}

function makeBarLayout(entry: VaultEntry, range: DateRange, dayWidthPx: number): BarLayout | null {
  const { start, due } = entryDateBounds(entry)
  const bounds = clampBarBounds(start, due)
  if (!bounds) return null
  const xPx = ((bounds.start - range.startMs) / MS_PER_DAY) * dayWidthPx
  const widthPx = Math.max(((bounds.end - bounds.start) / MS_PER_DAY) * dayWidthPx, dayWidthPx)
  return { entry, startMs: bounds.start, endMs: bounds.end, xPx, widthPx }
}

function compareLaneKeys(a: string, b: string, unsetLast: boolean): number {
  if (unsetLast) {
    if (a === '' && b !== '') return 1
    if (b === '' && a !== '') return -1
  }
  return a.localeCompare(b)
}

export function layoutBars(
  entries: VaultEntry[],
  range: DateRange,
  groupBy: ViewGroupBy,
  dayWidthPx: number,
  unsetLabel: string,
): { lanes: LaneGroup[]; undatedCount: number } {
  const field = normalizeBoardField(groupBy.property)
  const lanesByKey = new Map<string, LaneGroup>()
  let undatedCount = 0
  for (const entry of entries) {
    const bar = makeBarLayout(entry, range, dayWidthPx)
    if (!bar) {
      undatedCount += 1
      continue
    }
    const laneValues = readLaneValues(entry, field)
    const targets = laneValues.length > 0 ? laneValues : ['']
    for (const value of targets) {
      const key = value
      let lane = lanesByKey.get(key)
      if (!lane) {
        lane = { key, label: key === '' ? unsetLabel : key, isUnset: key === '', bars: [] }
        lanesByKey.set(key, lane)
      }
      lane.bars.push(bar)
    }
  }
  const lanes = [...lanesByKey.values()].sort((a, b) => compareLaneKeys(a.key, b.key, true))
  return { lanes, undatedCount }
}

export function pixelToDayOffset(px: number, dayWidthPx: number): number {
  return Math.round(px / dayWidthPx)
}

export function isoDateForOffsetMs(rangeStartMs: number, dayOffset: number): string {
  const ms = rangeStartMs + dayOffset * MS_PER_DAY
  const date = new Date(ms)
  const year = date.getUTCFullYear()
  const month = String(date.getUTCMonth() + 1).padStart(2, '0')
  const day = String(date.getUTCDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

export function isoDateForAbsoluteMs(ms: number): string {
  const date = new Date(startOfDayUtc(ms))
  const year = date.getUTCFullYear()
  const month = String(date.getUTCMonth() + 1).padStart(2, '0')
  const day = String(date.getUTCDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

export function buildTimelineLayout(
  entries: VaultEntry[],
  groupBy: ViewGroupBy,
  dayWidthPx: number,
  unsetLabel: string,
  today: Date = new Date(),
): TimelineLayout {
  const range = dateRangeFor(entries, today)
  const { lanes, undatedCount } = layoutBars(entries, range, groupBy, dayWidthPx, unsetLabel)
  return { range, lanes, undatedCount }
}

export const MS_PER_TIMELINE_DAY = MS_PER_DAY
