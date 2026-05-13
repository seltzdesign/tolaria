/**
 * Date-or-datetime values for task frontmatter fields (`due`, `start`, `completed`).
 *
 * Mirrors the Rust [`DateOrDateTime`](../../../src-tauri/src/vault/date_or_datetime.rs):
 * accepts either an ISO 8601 date (`YYYY-MM-DD`) or a full RFC 3339 datetime
 * (`YYYY-MM-DDTHH:MM:SS±HH:MM` / `...Z`). A datetime without an explicit offset
 * is treated as the system's local timezone at parse time, then materialized
 * with the corresponding fixed offset so the round-trip is stable.
 *
 * The string in `VaultEntry.properties` is the source of truth on disk; this
 * helper is for the editor cells to parse/format without re-implementing the
 * rules in three places.
 */

export type DateOrDateTime =
  | { kind: 'date'; date: string }
  | { kind: 'datetime'; iso: string }

const DATE_RE = /^(\d{4})-(\d{2})-(\d{2})$/
const RFC3339_RE =
  /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(Z|[+-]\d{2}:\d{2})$/
const NAIVE_DT_RE = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})$/

function isValidYmd(y: number, m: number, d: number): boolean {
  if (m < 1 || m > 12) return false
  if (d < 1 || d > 31) return false
  const probe = new Date(Date.UTC(y, m - 1, d))
  return (
    probe.getUTCFullYear() === y &&
    probe.getUTCMonth() === m - 1 &&
    probe.getUTCDate() === d
  )
}

function isValidTime(h: number, m: number, s: number): boolean {
  return h >= 0 && h <= 23 && m >= 0 && m <= 59 && s >= 0 && s <= 59
}

function pad2(n: number): string {
  return n < 10 ? `0${n}` : `${n}`
}

function formatLocalOffset(totalMinutes: number): string {
  const sign = totalMinutes >= 0 ? '+' : '-'
  const abs = Math.abs(totalMinutes)
  return `${sign}${pad2(Math.floor(abs / 60))}:${pad2(abs % 60)}`
}

function normalizeRfc3339(iso: string): string | null {
  const m = RFC3339_RE.exec(iso)
  if (!m) return null
  const [, y, mo, d, h, mi, s, offset] = m
  if (!isValidYmd(Number(y), Number(mo), Number(d))) return null
  if (!isValidTime(Number(h), Number(mi), Number(s))) return null
  const offsetNormalized = offset === 'Z' ? '+00:00' : offset
  if (offsetNormalized !== 'Z') {
    const [oh, om] = offsetNormalized.slice(1).split(':').map(Number)
    if (oh > 23 || om > 59) return null
  }
  return `${y}-${mo}-${d}T${h}:${mi}:${s}${offsetNormalized}`
}

function naiveToLocalRfc3339(iso: string): string | null {
  const m = NAIVE_DT_RE.exec(iso)
  if (!m) return null
  const [, y, mo, d, h, mi, s] = m
  if (!isValidYmd(Number(y), Number(mo), Number(d))) return null
  if (!isValidTime(Number(h), Number(mi), Number(s))) return null
  const localProbe = new Date(
    Number(y),
    Number(mo) - 1,
    Number(d),
    Number(h),
    Number(mi),
    Number(s),
  )
  const offsetMin = -localProbe.getTimezoneOffset()
  return `${y}-${mo}-${d}T${h}:${mi}:${s}${formatLocalOffset(offsetMin)}`
}

export function parseDateOrDateTime(raw: string): DateOrDateTime | null {
  const s = raw.trim()
  if (!s) return null
  const dateMatch = DATE_RE.exec(s)
  if (dateMatch) {
    const [, y, m, d] = dateMatch
    if (!isValidYmd(Number(y), Number(m), Number(d))) return null
    return { kind: 'date', date: `${y}-${m}-${d}` }
  }
  const rfc = normalizeRfc3339(s)
  if (rfc) return { kind: 'datetime', iso: rfc }
  const naive = naiveToLocalRfc3339(s)
  if (naive) return { kind: 'datetime', iso: naive }
  return null
}

export function formatDateOrDateTime(v: DateOrDateTime): string {
  return v.kind === 'date' ? v.date : v.iso
}

export function toNaiveDate(v: DateOrDateTime): string {
  if (v.kind === 'date') return v.date
  return v.iso.slice(0, 10)
}

export function hasTime(v: DateOrDateTime): boolean {
  return v.kind === 'datetime'
}
