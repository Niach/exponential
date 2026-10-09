// EXP-484/688/909/944: the agent's rate-limit windows, bound onto the ONE
// `UsageWindows` of `@exp/ui` (full | mini; the account picker's hover preview
// draws `density="hover"` off `AccountOption.limits` itself). Every rule (the
// titles, the three tones, the countdown wording, the as-of rule, which three
// windows the mini line keeps) lives in `lib/agent-usage.ts` and is
// hand-mirrored on iOS, Android and the desktop IDE.
import type { DeviceAgentUsage, DeviceUsageWindow } from "@/db/schema"
import { UsageWindows, type UsageWindow } from "@exp/ui"
import {
  formatResetCountdown,
  miniWindows,
  severity,
  usageAge,
  usageGroups,
} from "@/lib/agent-usage"

/** Full: every reported window in `usageGroups` order (no group headings ×4:
 *  every window names itself, so the groups only order the rows). */
export function fullUsageWindows(usage: DeviceAgentUsage, now: Date): UsageWindow[] {
  return usageGroups(usage, now).flatMap((group) =>
    group.cards.map((card) => ({
      key: card.key,
      label: card.title,
      percent: card.percent,
      tone: card.severity,
      caption: card.caption,
    }))
  )
}

/** EXP-944: only the two WINDOWS carry a reset under their mini bar; the
 *  per-model bar rides the weekly reset, so repeating it says the same time
 *  twice. No clock = bars only. */
function miniCaption(window: DeviceUsageWindow, now: Date | undefined): string | null {
  if (!now) return null
  if (window.key !== `session` && window.key !== `weekly`) return null
  return formatResetCountdown(window.resetsAt, now)
}

/** Mini: up to three windows on one line, wire labels (`5h Week Fable`). */
export function miniUsageWindows(
  usage: DeviceAgentUsage | null | undefined,
  now?: Date
): UsageWindow[] {
  return miniWindows(usage).map((window) => ({
    key: window.key,
    label: window.label,
    percent: window.percent,
    tone: severity(window.percent),
    caption: miniCaption(window, now),
  }))
}

export function AgentUsageWindows({
  usage,
  now,
  density = `full`,
  stale,
  className,
}: {
  usage: DeviceAgentUsage | null | undefined
  /** Required for `full`; on `mini` it adds the reset captions. */
  now?: Date
  density?: `full` | `mini`
  /** Mini only: the caller already captions the age beside the line. */
  stale?: boolean
  className?: string
}) {
  if (!usage) return null
  if (density === `mini`) {
    return (
      <UsageWindows
        windows={miniUsageWindows(usage, now)}
        density="mini"
        stale={stale}
        className={className}
      />
    )
  }
  const clock = now ?? new Date()
  const age = usageAge(usage, clock)
  return (
    <UsageWindows
      windows={fullUsageWindows(usage, clock)}
      stale={age !== null}
      footnote={age}
      className={className}
    />
  )
}
