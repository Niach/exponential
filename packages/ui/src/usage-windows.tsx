import { cn } from "./cn"
import { Meter, type MeterTone } from "./meter"

// THE usage windows: every rate-limit window an agent login reported, drawn at
// one of three densities off the SAME rows (the app resolves the rows from
// `lib/agent-usage.ts`, which owns the titles, the countdowns and the tones,
// hand-mirrored ×4: desktop `usage_bar`, iOS `AgentUsageCards`, Android
// `AgentUsageBar`).
//
//   full   two lines per window: title left, caption (the reset) right, then
//          the meter with its percent. The run's usage overlay, under the
//          account the run spends.
//   mini   ONE row of up to three tiny meters, `label · meter · NN%`, the
//          caption (the reset) centred under its bar. The other accounts and
//          every login on the Devices page.
//   hover  a column of label · meter, no percent: the account picker's hover
//          preview and the touch sheet row.
//
// Staleness DIMS the block and the `footnote` says how old the numbers are;
// aged numbers still beat none, so they are never hidden.

export type UsageWindowsDensity = `full` | `mini` | `hover`

export interface UsageWindow {
  key: string
  /** `Current session` (full) or the short wire label `5h` / `week` (mini, hover). */
  label: string
  /** 0-100, already clamped. */
  percent: number
  /** Omitted = the percent's own tone (`usageTone`). */
  tone?: MeterTone
  /** full: right of the title; mini: under the bar. Hover never shows it. */
  caption?: string | null
}

/** EXP-909's thresholds: ≥ 75 warning, ≥ 95 danger (`severity` ×4). */
export function usageTone(percent: number): MeterTone {
  if (percent >= 95) return `danger`
  if (percent >= 75) return `warning`
  return `normal`
}

export function UsageWindows({
  windows,
  density = `full`,
  stale = false,
  footnote,
  className,
}: {
  windows: readonly UsageWindow[]
  density?: UsageWindowsDensity
  /** The numbers are old: dim the whole block. */
  stale?: boolean
  /** One muted line under a full block (`as of 2h ago`). */
  footnote?: string | null
  className?: string
}) {
  if (windows.length === 0) return null
  const tone = (window: UsageWindow) => window.tone ?? usageTone(window.percent)

  if (density === `hover`) {
    return (
      <div
        data-slot="usage-windows"
        data-density={density}
        className={cn(`flex w-full flex-col gap-1`, stale && `opacity-50`, className)}
      >
        {windows.map((window) => (
          <div key={window.key} className="flex items-center gap-1.5">
            <span className="w-8 shrink-0 truncate text-[10px] leading-none text-muted-foreground">
              {window.label}
            </span>
            <Meter value={window.percent} tone={tone(window)} className="h-1 min-w-8 flex-1" />
          </div>
        ))}
      </div>
    )
  }

  if (density === `mini`) {
    return (
      <div
        data-slot="usage-windows"
        data-density={density}
        className={cn(`flex min-w-0 items-start gap-3`, stale && `opacity-50`, className)}
      >
        {windows.map((window) => (
          <div key={window.key} className="flex min-w-0 flex-1 flex-col gap-0.5">
            <div className="flex min-w-0 items-center gap-1.5">
              <span className="shrink-0 text-[10px] text-muted-foreground">{window.label}</span>
              <Meter value={window.percent} tone={tone(window)} className="h-1 min-w-4 flex-1" />
              <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                {`${window.percent}%`}
              </span>
            </div>
            {window.caption && (
              <span className="truncate text-center text-[10px] text-muted-foreground/60">
                {window.caption}
              </span>
            )}
          </div>
        ))}
      </div>
    )
  }

  return (
    <div
      data-slot="usage-windows"
      data-density={density}
      className={cn(`space-y-1.5`, stale && `opacity-50`, className)}
    >
      {windows.map((window) => (
        <div key={window.key} className="space-y-1">
          <div className="flex items-baseline gap-2">
            <span className="min-w-0 flex-1 truncate text-xs">{window.label}</span>
            {window.caption && (
              <span className="min-w-0 shrink-0 truncate text-[11px] text-muted-foreground">
                {window.caption}
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <Meter value={window.percent} tone={tone(window)} className="min-w-0 flex-1" />
            <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
              {`${window.percent}%`}
            </span>
          </div>
        </div>
      ))}
      {footnote && <p className="text-[11px] text-muted-foreground">{footnote}</p>}
    </div>
  )
}
