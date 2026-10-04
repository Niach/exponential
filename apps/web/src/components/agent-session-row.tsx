import type { PastRunRow } from "@/hooks/use-agents-data"
import type { SessionDisplayState } from "@/lib/coding-session-display"
import { relativeTime } from "@/components/comment-rows/format"
import {
  AgentWorkingMark,
  LiveDot,
  type LiveDotTone,
  type SessionDotTone,
} from "@exp/ui"
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

/** EXP-1184: the run state's dot tone — a working run's is never drawn as a
 *  dot (it wears the working mark), so it maps onto the live green. */
export const SESSION_DOT_TONE_BY_STATE: Record<
  SessionDisplayState,
  SessionDotTone
> = {
  working: `running`,
  needs_input: `needs_input`,
  review: `review`,
  done: `done`,
}

/** The row's state mark. EXP-862: the colours are the shared session-dot
 * table (`SESSION_DOT_CLASS`, the desktop's `queries::session_dot_tone`),
 * drawn through the `LiveDot` primitive. EXP-1184: while the agent WORKS the
 * dot gives way to the agent's working mark (Claude's stepped spark) — the
 * same mark the sidebar's Running rows wear. */
export function RunningIndicator({
  state,
  agent,
  paused = false,
  working = false,
}: {
  state: SessionDisplayState
  /** The run's `coding_sessions.agent` — whose working mark to draw. */
  agent?: string | null
  /** EXP-550: the host machine is offline — a steady grey dot. */
  paused?: boolean
  /** EXP-848: the agent is executing a turn on a live row
   * (`sessionRowIsWorking`) — the ONLY thing that animates. */
  working?: boolean
}) {
  if (paused) return <LiveDot tone={LIVE_DOT_TONE_BY_SESSION_TONE.muted} />
  if (working) return <AgentWorkingMark agent={agent} />
  return (
    <LiveDot
      tone={LIVE_DOT_TONE_BY_SESSION_TONE[SESSION_DOT_TONE_BY_STATE[state]]}
    />
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
