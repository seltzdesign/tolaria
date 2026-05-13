import { describe, expect, it } from 'vitest'
import {
  formatDateOrDateTime,
  hasTime,
  parseDateOrDateTime,
  toNaiveDate,
} from './dateOrDateTime'

describe('parseDateOrDateTime', () => {
  it('parses date-only as date', () => {
    const v = parseDateOrDateTime('2026-05-20')
    expect(v).toEqual({ kind: 'date', date: '2026-05-20' })
    expect(hasTime(v!)).toBe(false)
    expect(toNaiveDate(v!)).toBe('2026-05-20')
  })

  it('parses datetime with Z offset and normalizes to +00:00', () => {
    const v = parseDateOrDateTime('2026-05-20T14:00:00Z')
    expect(v).toEqual({ kind: 'datetime', iso: '2026-05-20T14:00:00+00:00' })
    expect(hasTime(v!)).toBe(true)
    expect(toNaiveDate(v!)).toBe('2026-05-20')
  })

  it('parses datetime with explicit positive offset', () => {
    const v = parseDateOrDateTime('2026-05-20T14:00:00+02:00')
    expect(v).toEqual({ kind: 'datetime', iso: '2026-05-20T14:00:00+02:00' })
  })

  it('parses datetime with explicit negative offset', () => {
    const v = parseDateOrDateTime('2026-05-20T14:00:00-08:00')
    expect(v).toEqual({ kind: 'datetime', iso: '2026-05-20T14:00:00-08:00' })
  })

  it('parses naive datetime, promoting to a real offset from local TZ', () => {
    const v = parseDateOrDateTime('2026-05-20T14:00:00')
    expect(v?.kind).toBe('datetime')
    if (v?.kind !== 'datetime') return
    expect(v.iso.startsWith('2026-05-20T14:00:00')).toBe(true)
    expect(/[+-]\d{2}:\d{2}$/.test(v.iso)).toBe(true)
    expect(toNaiveDate(v)).toBe('2026-05-20')
  })

  it('rejects invalid input', () => {
    expect(parseDateOrDateTime('not a date')).toBeNull()
    expect(parseDateOrDateTime('2026-13-01')).toBeNull()
    expect(parseDateOrDateTime('')).toBeNull()
    expect(parseDateOrDateTime('2026/05/20')).toBeNull()
    expect(parseDateOrDateTime('2026-02-30')).toBeNull()
  })

  it('trims whitespace', () => {
    const v = parseDateOrDateTime('  2026-05-20  ')
    expect(v).toEqual({ kind: 'date', date: '2026-05-20' })
  })

  it('round-trips a date through format', () => {
    const original = '2026-05-20'
    const parsed = parseDateOrDateTime(original)
    expect(formatDateOrDateTime(parsed!)).toBe(original)
  })

  it('round-trips a datetime with explicit offset through format', () => {
    const original = '2026-05-20T14:00:00+02:00'
    const parsed = parseDateOrDateTime(original)
    expect(formatDateOrDateTime(parsed!)).toBe(original)
  })

  it('Z normalizes to +00:00 on round-trip', () => {
    const parsed = parseDateOrDateTime('2026-05-20T14:00:00Z')
    expect(formatDateOrDateTime(parsed!)).toBe('2026-05-20T14:00:00+00:00')
  })

  it('toNaiveDate drops the time component', () => {
    const v = parseDateOrDateTime('2026-05-20T23:59:59+02:00')
    expect(toNaiveDate(v!)).toBe('2026-05-20')
  })

  it('rejects datetime with absurd offset components', () => {
    expect(parseDateOrDateTime('2026-05-20T14:00:00+99:00')).toBeNull()
  })
})
