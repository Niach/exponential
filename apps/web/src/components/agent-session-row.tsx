import type { PastRunRow } from "@/hooks/use-agents-data"
import { relativeTime } from "@/components/comment-rows/format"
import { type LiveDotTone, type SessionDotTone } from "@exp/ui"
import { pastRunByline, pastRunEndedAt } from "@/lib/past-runs"

// The session dot tones and the Recent caption, shared by every session list
// (EXP-874: the rows themselves live in `components/session-list-rows.tsx`,
// EXP-1208: led by the shared `AgentRunMark`, never a dot) plus the work-tab
// strip.

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
