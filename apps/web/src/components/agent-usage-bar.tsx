// EXP-484/688: the agent's rate-limit windows as CARDS, off the host
// machine's synced `devices.agent_usage`. One card per window, grouped by
// `usageGroups`: the current session, the weekly limits, then anything else
// the machine reported.
//
// Two surfaces render this and nothing else does: the mobile session view's
// "Usage" dialog and each agent's tab in the device-settings dialog
// (`compact`). Every rule (the titles, the three tones, the countdown
// wording, the stale dimming) lives in `lib/agent-usage.ts` and is
// hand-mirrored on iOS, Android and the desktop IDE. Change a string here,
// change it there.
import type { DeviceAgentUsage } from "@/db/schema"
import { usageGroups, type UsageSeverity } from "@/lib/agent-usage"
import { relativeTime } from "@/components/comment-rows/format"
import { AGENT_LABELS } from "@/components/launch-dialog/launch-options-pane"
import { cn } from "@/lib/utils"

const TONE: Record<UsageSeverity, string> = {
  normal: `bg-muted-foreground/60`,
  warning: `bg-amber-500`,
  danger: `bg-destructive`,
}

export function agentLabel(agent: string): string {
  return AGENT_LABELS[agent] ?? agent
}

/** Every window the machine reported, grouped. `compact` tightens the
 * spacing for the device-settings tabs, where the cards sit under an account
 * line rather than alone in a sheet. */
export function AgentUsageCards({
  usage,
  now,
  compact = false,
}: {
  usage: DeviceAgentUsage
  now: Date
  compact?: boolean
}) {
  const groups = usageGroups(usage, now)
  if (groups.length === 0) return null
  return (
    <div
      className={cn(
        compact ? `space-y-2` : `space-y-3`,
        usage.stale && `opacity-50`
      )}
    >
      {groups.map((group) => (
        <div key={group.key} className="space-y-1.5">
          {/* The session group needs no header — its single card is titled
              "Current session" already. */}
          {group.key !== `session` && (
            <p className="text-[11px] text-muted-foreground">{group.title}</p>
          )}
          {group.cards.map((card) => (
            <div
              key={card.key}
              className="space-y-1.5 rounded-xl border border-glass-stroke-card bg-glass-card px-3 py-2"
            >
              <div className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate text-xs">
                  {card.title}
                </span>
                <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
                  {`${card.percent}% used`}
                </span>
              </div>
              <span className="block h-1.5 w-full rounded-full bg-border/60">
                <span
                  className={`block h-full rounded-full ${TONE[card.severity]}`}
                  style={{ width: `${card.percent}%` }}
                />
              </span>
              {card.caption.length > 0 && (
                <p className="text-[11px] text-muted-foreground">
                  {card.caption}
                </p>
              )}
            </div>
          ))}
        </div>
      ))}
      {usage.stale && (
        <p className="text-[11px] text-muted-foreground">
          as of {relativeTime(usage.fetchedAt)}
        </p>
      )}
    </div>
  )
}
