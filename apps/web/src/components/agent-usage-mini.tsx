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
import type { DeviceAgentUsage } from "@/db/schema"
import { Meter } from "@exp/ui"
import { miniWindows, severity } from "@/lib/agent-usage"
import { cn } from "@/lib/utils"

export function UsageMini({
  usage,
  className,
}: {
  usage: DeviceAgentUsage | null | undefined
  className?: string
}) {
  const windows = miniWindows(usage)
  if (windows.length === 0) return null
  return (
    <div className={cn(`flex min-w-0 items-center gap-3`, className)}>
      {windows.map((window) => (
        <div
          key={window.key}
          className="flex min-w-0 flex-1 items-center gap-1.5"
        >
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
      ))}
    </div>
  )
}
