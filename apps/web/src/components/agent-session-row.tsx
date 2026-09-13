import type { PastRunRow } from "@/hooks/use-agents-data"
import type { SessionDisplayState } from "@/lib/coding-session-display"
import { relativeTime } from "@/components/comment-rows/format"
import { SESSION_DOT_CLASS } from "@/lib/session-dot"
import { pastRunByline, pastRunEndedAt } from "@/lib/past-runs"

// The session state dot and the Past caption, shared by every session list
// (EXP-874: the rows themselves live in `components/session-list-rows.tsx`)
// plus the sidebar's Pinned group and the work-tab strip.

/** The row's state dot. EXP-862: the colours are the shared session-dot table
 * (`lib/session-dot.ts`, the desktop's `queries::session_dot_tone`); the row
 * adds the EXP-848 ping, which fires only while the agent is WORKING. */
export function RunningIndicator({
  state,
  paused = false,
  working = false,
}: {
  state: SessionDisplayState
  /** EXP-550: the host machine is offline — a steady grey dot, no ping. */
  paused?: boolean
  /** EXP-848: the agent is executing a turn (`sessionRowIsWorking`) — the ONLY
   * thing that pings. A live-but-idle run draws the steady emerald dot. */
  working?: boolean
}) {
  if (paused) {
    return (
      <span
        className={`inline-flex size-2 rounded-full ${SESSION_DOT_CLASS.muted}`}
      />
    )
  }
  if (state !== `running`) {
    return (
      <span
        className={`inline-flex size-2 rounded-full ${SESSION_DOT_CLASS[state]}`}
      />
    )
  }
  if (!working) {
    return (
      <span
        className={`inline-flex size-2 rounded-full ${SESSION_DOT_CLASS.running}`}
      />
    )
  }
  return (
    <span className="relative flex size-2">
      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
      <span className="relative inline-flex size-2 rounded-full bg-emerald-500" />
    </span>
  )
}

/** EXP-746: the Past row's caption. The ORDER and the separator are the ×4
 * rule (lib/past-runs.ts); the relative time is this client's own formatter.
 * Lives here (EXP-739) so the Devices "Past" list and the chat page's "Past
 * chats" caption identically. */
export function pastRunRowByline(
  row: Pick<PastRunRow, `session` | `device`>
): string {
  // A row that stamped neither end nor heartbeat has no honest time to show
  // (0 would render as 1970), so that segment simply drops.
  const endedAt = pastRunEndedAt(row.session)
  return pastRunByline({
    deviceLabel: row.device.label ?? row.session.deviceLabel,
    relativeTime: endedAt > 0 ? relativeTime(new Date(endedAt)) : ``,
  })
}
