// Visualization primitives for the admin performance page (EXP-553) —
// minute-granularity siblings of -shared.tsx's DayBars, plus a utilization
// meter and formatters. Deliberately no chart library (same stance as
// DayBars).

import { BarStrip, type StripBar } from "./-shared"

/** Bar strip over the trailing `minutes` minutes (BarStrip from -shared,
 * EXP-759). Rows are keyed by the registry's UTC minute key
 * (`YYYY-MM-DDTHH:MM`); missing minutes render as the faint stub. `format`
 * renders the value ("312 requests", "45 ms"); `detail` adds a breakdown
 * line to the readout for that minute. */
export function MinuteBars({
  rows,
  minutes = 60,
  format,
  detail,
}: {
  rows: { minute: string; value: number }[]
  minutes?: number
  format: (value: number) => string
  detail?: (minute: string) => string | undefined
}) {
  const byMinute = new Map(rows.map((r) => [r.minute, r.value]))
  const nowMinute = Math.floor(Date.now() / 60_000)
  const bars: StripBar[] = []
  for (let i = minutes - 1; i >= 0; i--) {
    const key = new Date((nowMinute - i) * 60_000).toISOString().slice(0, 16)
    bars.push({
      key,
      label: `${key.slice(11)} UTC${i === 0 ? ` (now)` : ``}`,
      value: byMinute.get(key) ?? 0,
      detail: detail?.(key),
    })
  }
  return (
    <BarStrip bars={bars} heightClass="h-14" gapClass="gap-px" format={format} />
  )
}

/** Horizontal utilization bar. The caller decides when it's alarming —
 * `warn` flips the fill to destructive. */
export function Meter({
  label,
  value,
  max,
  display,
  warn = false,
}: {
  label: string
  value: number
  max: number
  /** Right-hand caption; defaults to `value / max`. */
  display?: string
  warn?: boolean
}) {
  const ratio = max > 0 ? Math.min(1, value / max) : 0
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-xs">
        <span className="text-muted-foreground">{label}</span>
        <span className="tabular-nums">
          {display ?? `${formatCount(value)} / ${formatCount(max)}`}
        </span>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-muted">
        <div
          className={warn ? `h-full bg-destructive` : `h-full bg-primary`}
          style={{ width: `${ratio * 100}%` }}
        />
      </div>
    </div>
  )
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return `—`
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${Math.round(bytes)} B`
}

export function formatMs(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return `—`
  if (ms >= 10_000) return `${(ms / 1000).toFixed(1)} s`
  if (ms >= 1) return `${Math.round(ms)} ms`
  return ms === 0 ? `0 ms` : `<1 ms`
}

export function formatUptime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return `—`
  const days = Math.floor(seconds / 86_400)
  const hours = Math.floor((seconds % 86_400) / 3_600)
  const mins = Math.floor((seconds % 3_600) / 60)
  if (days > 0) return `${days}d ${hours}h ${mins}m`
  if (hours > 0) return `${hours}h ${mins}m`
  return `${mins}m`
}

export function formatCount(value: number): string {
  if (!Number.isFinite(value)) return `—`
  return value.toLocaleString(`en-US`)
}
