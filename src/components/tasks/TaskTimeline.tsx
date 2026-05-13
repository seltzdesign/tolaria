import { memo, useCallback, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'
import { cn } from '@/lib/utils'
import type { VaultEntry, ViewFile } from '../../types'
import { translate, type AppLocale } from '../../lib/i18n'
import { trackEvent } from '../../lib/telemetry'
import {
  buildTimelineLayout,
  isoDateForAbsoluteMs,
  isoDateForOffsetMs,
  pixelToDayOffset,
  MS_PER_TIMELINE_DAY,
  type BarLayout,
  type DateRange,
} from '../../lib/tasks/timelineLayout'

type FrontmatterUpdate = (
  path: string,
  key: string,
  value: string | number | boolean | string[] | null,
) => void | Promise<void>

export interface TaskTimelineProps {
  view: ViewFile
  filteredEntries: VaultEntry[]
  selectedEntryPath?: string | null
  onSelectNote?: (entry: VaultEntry) => void
  onUpdateFrontmatter: FrontmatterUpdate
  locale: AppLocale
}

const DAY_WIDTH_PX = 40
const ROW_HEIGHT_PX = 36
const BAR_HEIGHT_PX = 22
const LANE_LABEL_WIDTH_PX = 160
const HEADER_HEIGHT_PX = 36
const EDGE_HANDLE_WIDTH_PX = 8
const DRAG_THRESHOLD_PX = 4

type DragMode = 'move' | 'resize-start' | 'resize-end'

interface DragState {
  entry: VaultEntry
  mode: DragMode
  startX: number
  startBarX: number
  startBarWidth: number
  originalStartMs: number | null
  originalDueMs: number | null
  totalDxPx: number
  moved: boolean
}

interface BarVisual {
  bar: BarLayout
  lane: { key: string; label: string; isUnset: boolean }
  rowY: number
}

function dayHeaders(range: DateRange, dayWidthPx: number): { x: number; label: string; isWeekStart: boolean; isToday: boolean }[] {
  const today = new Date()
  const todayUtcDay = Math.floor(Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate()) / MS_PER_TIMELINE_DAY)
  const headers: { x: number; label: string; isWeekStart: boolean; isToday: boolean }[] = []
  for (let i = 0; i < range.days; i += 1) {
    const ms = range.startMs + i * MS_PER_TIMELINE_DAY
    const date = new Date(ms)
    const dayOfWeek = date.getUTCDay()
    const dayOfMonth = date.getUTCDate()
    headers.push({
      x: i * dayWidthPx,
      label: dayOfWeek === 0 || dayOfMonth === 1 ? `${date.toLocaleString('en', { month: 'short' })} ${dayOfMonth}` : String(dayOfMonth),
      isWeekStart: dayOfWeek === 0,
      isToday: Math.floor(ms / MS_PER_TIMELINE_DAY) === todayUtcDay,
    })
  }
  return headers
}

function todayX(range: DateRange, dayWidthPx: number): number | null {
  const today = new Date()
  const todayMs = Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate())
  if (todayMs < range.startMs || todayMs > range.endMs) return null
  return ((todayMs - range.startMs) / MS_PER_TIMELINE_DAY) * dayWidthPx
}

function emitDateUpdates(state: DragState, dxPx: number, update: FrontmatterUpdate): 'start' | 'due' | 'both' | null {
  const days = pixelToDayOffset(dxPx, DAY_WIDTH_PX)
  if (days === 0) return null
  const dayMs = days * MS_PER_TIMELINE_DAY
  let changes: 'start' | 'due' | 'both' | null = null
  const writeStart = state.mode === 'move' || state.mode === 'resize-start'
  const writeDue = state.mode === 'move' || state.mode === 'resize-end'
  if (writeStart && state.originalStartMs !== null) {
    void update(state.entry.path, 'start', isoDateForAbsoluteMs(state.originalStartMs + dayMs))
    changes = 'start'
  }
  if (writeDue && state.originalDueMs !== null) {
    void update(state.entry.path, 'due', isoDateForAbsoluteMs(state.originalDueMs + dayMs))
    changes = changes === 'start' ? 'both' : 'due'
  }
  return changes
}

function readOriginalDateMs(entry: VaultEntry, key: 'start' | 'due'): number | null {
  const raw = entry.properties[key]
  if (typeof raw !== 'string' || !raw) return null
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(raw)
  if (!match) return null
  const [, year, month, day] = match
  return Date.UTC(Number(year), Number(month) - 1, Number(day))
}

function previewBarRect(bar: BarLayout, drag: DragState | null, dxPx: number): { xPx: number; widthPx: number } {
  if (!drag || drag.entry.path !== bar.entry.path) return { xPx: bar.xPx, widthPx: bar.widthPx }
  const days = pixelToDayOffset(dxPx, DAY_WIDTH_PX)
  const deltaPx = days * DAY_WIDTH_PX
  if (drag.mode === 'move') return { xPx: drag.startBarX + deltaPx, widthPx: drag.startBarWidth }
  if (drag.mode === 'resize-start') return { xPx: drag.startBarX + deltaPx, widthPx: Math.max(DAY_WIDTH_PX, drag.startBarWidth - deltaPx) }
  return { xPx: drag.startBarX, widthPx: Math.max(DAY_WIDTH_PX, drag.startBarWidth + deltaPx) }
}

interface TimelineHeaderProps {
  range: DateRange
  locale: AppLocale
}

function TimelineHeader({ range, locale }: TimelineHeaderProps) {
  const headers = useMemo(() => dayHeaders(range, DAY_WIDTH_PX), [range])
  return (
    <div
      className="sticky top-0 z-10 flex border-b border-border bg-muted/60 text-[11px] text-muted-foreground"
      style={{ height: HEADER_HEIGHT_PX, paddingLeft: LANE_LABEL_WIDTH_PX, minWidth: LANE_LABEL_WIDTH_PX + range.days * DAY_WIDTH_PX }}
      data-testid="timeline-header"
    >
      {headers.map((header) => (
        <div
          key={header.x}
          className={cn(
            'shrink-0 border-l border-border/60 px-1 leading-9',
            header.isWeekStart ? 'border-l-border' : null,
            header.isToday ? 'bg-accent/50 font-semibold text-foreground' : null,
          )}
          style={{ width: DAY_WIDTH_PX }}
          data-testid={`timeline-header-day-${isoDateForOffsetMs(range.startMs, Math.round(header.x / DAY_WIDTH_PX))}`}
          aria-label={isoDateForOffsetMs(range.startMs, Math.round(header.x / DAY_WIDTH_PX))}
        >
          {translate(locale, 'tasks.timeline.today').length > 0 && header.isToday ? translate(locale, 'tasks.timeline.today') : header.label}
        </div>
      ))}
    </div>
  )
}

export const TaskTimeline = memo(function TaskTimeline({
  view,
  filteredEntries,
  selectedEntryPath,
  onSelectNote,
  onUpdateFrontmatter,
  locale,
}: TaskTimelineProps) {
  const groupBy = useMemo(
    () => view.definition.groupBy ?? { property: 'assignee' },
    [view.definition.groupBy],
  )
  const unsetLabel = translate(locale, 'tasks.timeline.unsetLane')
  const layout = useMemo(
    () => buildTimelineLayout(filteredEntries, groupBy, DAY_WIDTH_PX, unsetLabel),
    [filteredEntries, groupBy, unsetLabel],
  )

  const totalRows = layout.lanes.reduce((sum, lane) => sum + lane.bars.length, 0)
  const visuals: BarVisual[] = []
  let rowCursor = 0
  for (const lane of layout.lanes) {
    for (const bar of lane.bars) {
      visuals.push({ bar, lane, rowY: rowCursor * ROW_HEIGHT_PX + (ROW_HEIGHT_PX - BAR_HEIGHT_PX) / 2 })
      rowCursor += 1
    }
  }

  const [drag, setDrag] = useState<DragState | null>(null)
  const dragRef = useRef<DragState | null>(null)

  const writeDrag = useCallback((next: DragState | null) => {
    dragRef.current = next
    setDrag(next)
  }, [])

  const handlePointerDown = useCallback(
    (event: ReactPointerEvent<SVGRectElement>, bar: BarLayout, mode: DragMode) => {
      event.preventDefault()
      const target = event.currentTarget as Element & { setPointerCapture?: (id: number) => void }
      target.setPointerCapture?.(event.pointerId)
      writeDrag({
        entry: bar.entry,
        mode,
        startX: event.clientX,
        startBarX: bar.xPx,
        startBarWidth: bar.widthPx,
        originalStartMs: readOriginalDateMs(bar.entry, 'start'),
        originalDueMs: readOriginalDateMs(bar.entry, 'due'),
        totalDxPx: 0,
        moved: false,
      })
    },
    [writeDrag],
  )

  const handlePointerMove = useCallback((event: ReactPointerEvent<SVGRectElement>) => {
    const current = dragRef.current
    if (!current) return
    const dx = event.clientX - current.startX
    if (!current.moved && Math.abs(dx) >= DRAG_THRESHOLD_PX) {
      writeDrag({ ...current, moved: true, totalDxPx: dx })
      return
    }
    if (current.moved) {
      writeDrag({ ...current, totalDxPx: dx })
    }
  }, [writeDrag])

  const handlePointerUp = useCallback(
    (event: ReactPointerEvent<SVGRectElement>) => {
      const current = dragRef.current
      if (!current) return
      const dx = event.clientX - current.startX
      if (!current.moved) {
        onSelectNote?.(current.entry)
        writeDrag(null)
        return
      }
      const changes = emitDateUpdates(current, dx, onUpdateFrontmatter)
      if (changes) trackEvent('task_dates_changed', { field: changes })
      writeDrag(null)
    },
    [onSelectNote, onUpdateFrontmatter, writeDrag],
  )

  if (filteredEntries.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        {translate(locale, 'tasks.timeline.emptyView')}
      </div>
    )
  }

  const canvasWidth = LANE_LABEL_WIDTH_PX + layout.range.days * DAY_WIDTH_PX
  const canvasHeight = Math.max(totalRows * ROW_HEIGHT_PX, ROW_HEIGHT_PX)
  const todayLineX = todayX(layout.range, DAY_WIDTH_PX)

  return (
    <div data-testid="task-timeline" className="flex h-full flex-col overflow-hidden">
      <div className="flex-1 overflow-auto">
        <div style={{ minWidth: canvasWidth }}>
          <TimelineHeader range={layout.range} locale={locale} />
          <div className="relative" style={{ minHeight: canvasHeight }}>
            <div
              className="absolute left-0 top-0 flex flex-col border-r border-border bg-muted/30 text-[12px]"
              style={{ width: LANE_LABEL_WIDTH_PX }}
              data-testid="timeline-lanes"
            >
              {visuals.map((visual, index) => {
                const isFirstInLane = visuals[index - 1]?.lane.key !== visual.lane.key
                return (
                  <div
                    key={`${visual.lane.key}:${visual.bar.entry.path}`}
                    className={cn('flex items-center px-2', isFirstInLane ? 'border-t border-border/60 font-medium' : 'text-muted-foreground')}
                    style={{ height: ROW_HEIGHT_PX }}
                    title={visual.lane.label}
                  >
                    {isFirstInLane ? visual.lane.label : ''}
                  </div>
                )
              })}
            </div>
            <svg
              role="presentation"
              width={canvasWidth - LANE_LABEL_WIDTH_PX}
              height={canvasHeight}
              style={{ marginLeft: LANE_LABEL_WIDTH_PX, display: 'block' }}
            >
              {todayLineX !== null ? (
                <line x1={todayLineX} x2={todayLineX} y1={0} y2={canvasHeight} stroke="currentColor" strokeOpacity={0.25} strokeDasharray="2 4" />
              ) : null}
              {visuals.map((visual) => {
                const isSelected = selectedEntryPath === visual.bar.entry.path
                const preview = previewBarRect(visual.bar, drag, drag && drag.entry.path === visual.bar.entry.path ? drag.totalDxPx : 0)
                return (
                  <g key={visual.bar.entry.path}>
                    <rect
                      data-testid={`timeline-bar-${visual.bar.entry.path}`}
                      data-entry-path={visual.bar.entry.path}
                      x={preview.xPx}
                      y={visual.rowY}
                      width={preview.widthPx}
                      height={BAR_HEIGHT_PX}
                      rx={4}
                      className={cn('cursor-grab fill-primary/80 stroke-primary stroke-1 hover:fill-primary', isSelected ? 'fill-primary' : null)}
                      onPointerDown={(event) => handlePointerDown(event, visual.bar, 'move')}
                      onPointerMove={handlePointerMove}
                      onPointerUp={handlePointerUp}
                    />
                    <rect
                      data-testid={`timeline-bar-handle-start-${visual.bar.entry.path}`}
                      x={preview.xPx}
                      y={visual.rowY}
                      width={EDGE_HANDLE_WIDTH_PX}
                      height={BAR_HEIGHT_PX}
                      className="cursor-ew-resize fill-transparent"
                      onPointerDown={(event) => handlePointerDown(event, visual.bar, 'resize-start')}
                      onPointerMove={handlePointerMove}
                      onPointerUp={handlePointerUp}
                    />
                    <rect
                      data-testid={`timeline-bar-handle-end-${visual.bar.entry.path}`}
                      x={preview.xPx + preview.widthPx - EDGE_HANDLE_WIDTH_PX}
                      y={visual.rowY}
                      width={EDGE_HANDLE_WIDTH_PX}
                      height={BAR_HEIGHT_PX}
                      className="cursor-ew-resize fill-transparent"
                      onPointerDown={(event) => handlePointerDown(event, visual.bar, 'resize-end')}
                      onPointerMove={handlePointerMove}
                      onPointerUp={handlePointerUp}
                    />
                    <text
                      x={preview.xPx + 6}
                      y={visual.rowY + BAR_HEIGHT_PX / 2 + 4}
                      fontSize="11"
                      className="pointer-events-none select-none fill-primary-foreground"
                    >
                      {visual.bar.entry.title || visual.bar.entry.filename}
                    </text>
                  </g>
                )
              })}
            </svg>
          </div>
        </div>
      </div>
      {layout.undatedCount > 0 ? (
        <div
          className="border-t border-border bg-muted/30 px-3 py-1 text-xs text-muted-foreground"
          data-testid="timeline-undated-footer"
        >
          {translate(locale, 'tasks.timeline.noDatesFooter', { count: layout.undatedCount })}
        </div>
      ) : null}
    </div>
  )
})
