import { describe, expect, it } from 'vitest'
import type { VaultEntry } from '../../types'
import {
  buildTimelineLayout,
  dateRangeFor,
  isoDateForOffsetMs,
  isoDateForAbsoluteMs,
  layoutBars,
  pixelToDayOffset,
  MS_PER_TIMELINE_DAY,
} from './timelineLayout'

function entry(overrides: Partial<VaultEntry> = {}): VaultEntry {
  return {
    path: `/vault/${overrides.filename ?? 'note.md'}`,
    filename: 'note.md',
    title: 'Note',
    isA: 'task',
    aliases: [],
    belongsTo: [],
    relatedTo: [],
    status: null,
    archived: false,
    modifiedAt: null,
    createdAt: null,
    fileSize: 0,
    snippet: '',
    wordCount: 0,
    relationships: {},
    icon: null,
    color: null,
    order: null,
    sidebarLabel: null,
    template: null,
    sort: null,
    view: null,
    visible: null,
    organized: false,
    favorite: false,
    favoriteIndex: null,
    listPropertiesDisplay: [],
    outgoingLinks: [],
    properties: {},
    hasH1: false,
    ...overrides,
  }
}

const TODAY = new Date(Date.UTC(2026, 4, 1)) // 2026-05-01

describe('dateRangeFor', () => {
  it('falls back to today-centered range when no dates are present', () => {
    const range = dateRangeFor([entry()], TODAY)
    expect(isoDateForAbsoluteMs(range.startMs)).toBe('2026-04-24')
    expect(isoDateForAbsoluteMs(range.endMs)).toBe('2026-05-31')
  })

  it('pads the date range by 3 days on each side when dates are present', () => {
    const entries = [
      entry({ properties: { start: '2026-05-10', due: '2026-05-20' } }),
    ]
    const range = dateRangeFor(entries, TODAY)
    expect(isoDateForAbsoluteMs(range.startMs)).toBe('2026-05-07')
    expect(isoDateForAbsoluteMs(range.endMs)).toBe('2026-05-23')
  })

  it('expands to cover all entries when multiple are present', () => {
    const entries = [
      entry({ properties: { start: '2026-05-10', due: '2026-05-12' } }),
      entry({ properties: { start: '2026-06-01', due: '2026-06-05' } }),
    ]
    const range = dateRangeFor(entries, TODAY)
    expect(isoDateForAbsoluteMs(range.startMs)).toBe('2026-05-07')
    expect(isoDateForAbsoluteMs(range.endMs)).toBe('2026-06-08')
  })
})

describe('layoutBars', () => {
  it('produces one bar per entry with correct x and width', () => {
    const a = entry({ path: '/a.md', properties: { start: '2026-05-10', due: '2026-05-15' } })
    const range = dateRangeFor([a], TODAY)
    const { lanes } = layoutBars([a], range, { property: 'status' }, 40, '(unset)')
    expect(lanes).toHaveLength(1)
    const bar = lanes[0].bars[0]
    expect(bar.entry.path).toBe('/a.md')
    expect(bar.xPx).toBe(3 * 40)
    expect(bar.widthPx).toBe(5 * 40)
  })

  it('uses a minimum one-day width for same-day bars', () => {
    const a = entry({ path: '/a.md', properties: { due: '2026-05-10' } })
    const range = dateRangeFor([a], TODAY)
    const { lanes } = layoutBars([a], range, { property: 'status' }, 40, '(unset)')
    expect(lanes[0].bars[0].widthPx).toBe(40)
  })

  it('omits entries with no dates and reports them as undated', () => {
    const a = entry({ path: '/a.md', properties: { start: '2026-05-10', due: '2026-05-12' } })
    const b = entry({ path: '/b.md' })
    const range = dateRangeFor([a, b], TODAY)
    const { lanes, undatedCount } = layoutBars([a, b], range, { property: 'status' }, 40, '(unset)')
    expect(undatedCount).toBe(1)
    expect(lanes.flatMap((lane) => lane.bars.map((bar) => bar.entry.path))).toEqual(['/a.md'])
  })

  it('groups by status and creates an unset lane when status is null', () => {
    const a = entry({ path: '/a.md', status: 'Open', properties: { due: '2026-05-10' } })
    const b = entry({ path: '/b.md', status: null, properties: { due: '2026-05-12' } })
    const range = dateRangeFor([a, b], TODAY)
    const { lanes } = layoutBars([a, b], range, { property: 'status' }, 40, '(unset)')
    expect(lanes.map((lane) => lane.key)).toEqual(['Open', ''])
    const unset = lanes.find((lane) => lane.isUnset)
    expect(unset?.bars.map((bar) => bar.entry.path)).toEqual(['/b.md'])
  })

  it('uses the first assignee value when groupBy is the assignee field', () => {
    const a = entry({
      path: '/a.md',
      properties: { due: '2026-05-10' },
      relationships: { assignee: ['[[Armin]]'] },
    })
    const range = dateRangeFor([a], TODAY)
    const { lanes } = layoutBars([a], range, { property: 'assignee' }, 40, '(unset)')
    expect(lanes.map((lane) => lane.label)).toEqual(['Armin'])
  })

  it('honors the note. prefix on the group-by property', () => {
    const a = entry({ path: '/a.md', status: 'Open', properties: { due: '2026-05-10' } })
    const range = dateRangeFor([a], TODAY)
    const { lanes } = layoutBars([a], range, { property: 'note.status' }, 40, '(unset)')
    expect(lanes.map((lane) => lane.label)).toEqual(['Open'])
  })
})

describe('pixelToDayOffset', () => {
  it('rounds to the nearest day', () => {
    expect(pixelToDayOffset(0, 40)).toBe(0)
    expect(pixelToDayOffset(39, 40)).toBe(1)
    expect(pixelToDayOffset(60, 40)).toBe(2)
    expect(pixelToDayOffset(-10, 40)).toBe(-0)
  })
})

describe('isoDateForOffsetMs', () => {
  it('returns the ISO date for a day offset from the range start', () => {
    const rangeStart = Date.UTC(2026, 4, 1)
    expect(isoDateForOffsetMs(rangeStart, 0)).toBe('2026-05-01')
    expect(isoDateForOffsetMs(rangeStart, 3)).toBe('2026-05-04')
    expect(isoDateForOffsetMs(rangeStart, -2)).toBe('2026-04-29')
  })
})

describe('buildTimelineLayout', () => {
  it('combines range + lanes + undated count in one helper', () => {
    const entries = [
      entry({ path: '/a.md', status: 'Open', properties: { start: '2026-05-10', due: '2026-05-12' } }),
      entry({ path: '/b.md', status: null, properties: {} }),
    ]
    const layout = buildTimelineLayout(entries, { property: 'status' }, 40, '(unset)', TODAY)
    expect(layout.range.days).toBeGreaterThan(0)
    expect(layout.lanes).toHaveLength(1)
    expect(layout.lanes[0].bars).toHaveLength(1)
    expect(layout.undatedCount).toBe(1)
  })
})

it('one timeline day equals MS_PER_DAY constant for predictable math', () => {
  expect(MS_PER_TIMELINE_DAY).toBe(86_400_000)
})
