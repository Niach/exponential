// EXP-909: the SHORT usage line — up to three tiny meters in ONE row, each
// `label · meter · NN%` off `miniWindows` (the five-hour window, the rolling
// week, the first per-model one; wire labels, so `5h Week Fable` fits where
// `Current session` would not).
//
// It is the line under an account row that is NOT the run's own: the usage
// overlay's other accounts, every login under a device on the Devices page,
// and (EXP-872, next) the account picker's hover preview — so it is a
// standalone piece with no surface of its own, not a variant of `UsageWindows`.
// The full two-line form stays the overlay header's business.
//
// Hand-mirrored ×4: desktop `usage_bar::render_usage_mini`, iOS
// `AgentUsageMini`, Android `AgentUsageMini`. Every rule (which windows, the
// labels, the tones) lives in `lib/agent-usage.ts`.
import type { DeviceAgentUsage, DeviceUsageWindow } from "@/db/schema"
import { Meter } from "@exp/ui"
import { formatResetCountdown, miniWindows, severity } from "@/lib/agent-usage"
import { cn } from "@/lib/utils"

/** EXP-944: only the two WINDOWS carry a reset under their bar. The per-model
 * bar (`model:…`, "Fable") rides the weekly window's reset, so repeating it
 * would say the same time twice. */
function windowReset(
  window: DeviceUsageWindow,
  now: Date | undefined
): string | null {
  if (!now) return null
  if (window.key !== `session` && window.key !== `weekly`) return null
  return formatResetCountdown(window.resetsAt, now)
}

export function UsageMini({
  usage,
  now,
  className,
}: {
  usage: DeviceAgentUsage | null | undefined
  /** EXP-944: pass a clock to caption the 5h and Week bars with when they
   *  reset (`resets in 2h 14m`, the desktop's wording). Omitted = bars only,
   *  which is what the tight surfaces (the usage overlay's other accounts,
   *  the account picker's preview) want. */
  now?: Date
  className?: string
}) {
  const windows = miniWindows(usage)
  if (windows.length === 0) return null
  return (
    <div className={cn(`flex min-w-0 items-start gap-3`, className)}>
      {windows.map((window) => {
        const reset = windowReset(window, now)
        return (
          <div key={window.key} className="flex min-w-0 flex-1 flex-col gap-0.5">
            <div className="flex min-w-0 items-center gap-1.5">
              <span className="shrink-0 text-[10px] text-muted-foreground">
                {window.label}
              </span>
              <Meter
                value={window.percent}
                tone={severity(window.percent)}
                className="h-1 min-w-4 flex-1"
              />
              <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                {`${window.percent}%`}
              </span>
            </div>
            {reset && (
              <span className="truncate text-center text-[10px] text-muted-foreground/60">
                {reset}
              </span>
            )}
          </div>
        )
      })}
    </div>
  )
}
