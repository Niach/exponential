import type { PastRunRow } from "@/hooks/use-agents-data"
import type { SessionDisplayState } from "@/lib/coding-session-display"
import { relativeTime } from "@/components/comment-rows/format"
import { LiveDot, type LiveDotTone, type SessionDotTone } from "@exp/ui"
import { pastRunByline, pastRunEndedAt } from "@/lib/past-runs"

// The session state dot and the Recent caption, shared by every session list
// (EXP-874: the rows themselves live in `components/session-list-rows.tsx`)
// plus the sidebar's Pinned group and the work-tab strip.

/** EXP-887: the session tones, as `LiveDot` tones. The COLOURS still live in
 * the ×4 `SESSION_DOT_CLASS` table — this only says which of the primitive's
 * tones paints each one, and `lib/session-dot.test.ts` locks the two together
 * so a colour can never drift between the table and the dot that draws it. */
export const LIVE_DOT_TONE_BY_SESSION_TONE: Record<
  SessionDotTone,
  LiveDotTone
> = {
  running: `live`,
  review: `live`,
  needs_input: `attention`,
  done: `done`,
  muted: `idle`,
}

/** The row's state dot. EXP-862: the colours are the shared session-dot table
 * (`SESSION_DOT_CLASS`, the desktop's `queries::session_dot_tone`), drawn here
 * through the `LiveDot` primitive; the row adds the EXP-848 ping, which fires
 * only while the agent is WORKING. */
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
  if (paused) return <LiveDot tone={LIVE_DOT_TONE_BY_SESSION_TONE.muted} />
  if (state !== `running`)
    return <LiveDot tone={LIVE_DOT_TONE_BY_SESSION_TONE[state]} />
  return (
    <LiveDot tone={LIVE_DOT_TONE_BY_SESSION_TONE.running} ping={working} />
  )
}

/** EXP-746: the Recent row's caption. The ORDER and the separator are the ×4
 * rule (lib/past-runs.ts); the relative time is this client's own formatter.
 * Lives here (EXP-739) so the Agent page's "Recent" list and the chat page's "Past
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
