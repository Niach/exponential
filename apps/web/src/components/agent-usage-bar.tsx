// EXP-484/688/909: the agent's rate-limit windows, FULL form — every window
// the host machine reported (`usageGroups`: the current session, the weekly
// limits, then anything else), each as TWO lines:
//
//   Current session                        resets in 1h 4m
//   ▓▓░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   9%
//
// One surface renders it — the run's usage overlay, under the account it runs
// on. The other accounts, and every login on the Devices page, get the short
// three-bar `UsageMini` instead; there is no `compact` and no `dense` variant
// any more, because there is no third rhythm to be in.
//
// EXP-909: the bar is the shared `Meter` from `@exp/ui`, the same primitive the
// Context block and the mini line draw — the hand-rolled span with its own tone
// map is gone. Staleness DIMS the block and captions it (`usageAge`); it never
// hides it, since numbers with an age on them are still the best answer anyone
// has.
//
// Every rule (the titles, the three tones, the countdown wording, the as-of
// rule) lives in `lib/agent-usage.ts` and is hand-mirrored on iOS, Android and
// the desktop IDE. Change a string here, change it there.
import type { DeviceAgentUsage } from "@/db/schema"
import { Meter } from "@exp/ui"
import { usageAge, usageGroups } from "@/lib/agent-usage"
import { cn } from "@/lib/utils"

export function UsageWindows({
  usage,
  now,
  className,
}: {
  usage: DeviceAgentUsage
  now: Date
  className?: string
}) {
  const groups = usageGroups(usage, now)
  if (groups.length === 0) return null
  const age = usageAge(usage, now)
  return (
    <div className={cn(`space-y-2`, age && `opacity-50`, className)}>
      {groups.map((group) => (
        <div key={group.key} className="space-y-1.5">
          {/* EXP-909: no group headings ×4 — every window names itself
              ("Current session" / "All models" / "<Label> only", or its wire
              label), so the groups only order the rows. */}
          {group.cards.map((card) => (
            <div key={card.key} className="space-y-1">
              <div className="flex items-baseline gap-2">
                <span className="min-w-0 flex-1 truncate text-xs">
                  {card.title}
                </span>
                {card.caption.length > 0 && (
                  <span className="min-w-0 shrink-0 truncate text-[11px] text-muted-foreground">
                    {card.caption}
                  </span>
                )}
              </div>
              <div className="flex items-center gap-2">
                <Meter
                  value={card.percent}
                  tone={card.severity}
                  className="min-w-0 flex-1"
                />
                <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
                  {`${card.percent}%`}
                </span>
              </div>
            </div>
          ))}
        </div>
      ))}
      {age && <p className="text-[11px] text-muted-foreground">{age}</p>}
    </div>
  )
}
