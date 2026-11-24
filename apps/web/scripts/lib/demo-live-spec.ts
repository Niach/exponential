/**
 * The seeded LIVE sessions' spec, PURE (no database): the seed and the stub
 * write it (lib/demo-live-sessions.ts) and a test replays it through the
 * product's own rules (demo-live-spec.test.ts).
 */
import type { CodingSessionBlocked } from "@exp/db-schema/domain"
import { DEMO_CHAT_IDS, DEMO_SESSION_ID } from "../screenshot-demo"

export interface LiveSessionSpec {
  id: string
  status: `running` | `idle`
  /** Device-written turn state: the list spinner. */
  busy: boolean
  startedMinutesAgo: number
  /** The stub's feed for this row ends on an unanswered question
   *  (screenshot-desktop.ts) and the chat carries the question card, so
   *  the row says so the way a real device stamps it: `needs_input`. */
  asks?: boolean
  /** #EXP-2105: its device (the build server) has been away for hours, so
   *  the row is what the watchdog left: the run cut mid-turn (`flagOffline`:
   *  idle, not busy, `device_offline` + resume) and, `offlineMinutes` later,
   *  on a person (`flagOfflineTooLong`). Never `busy`: production holds a
   *  busy row on a silent device for one watchdog tick, not for hours, and
   *  the chat header read "working" over the transcript's offline line. */
  away?: boolean
}

/** The build server's last heartbeat (the seed's `last_seen_at`). */
export const DEMO_SERVER_AWAY_MINUTES = 5 * 60
/** The watchdog's flag: the online window after that beat, plus its tick. */
const DEMO_AWAY_FLAGGED_MINUTES_AGO = DEMO_SERVER_AWAY_MINUTES - 2

export const DEMO_LIVE_SESSIONS: readonly LiveSessionSpec[] = [
  { id: DEMO_CHAT_IDS.coldStart, status: `running`, busy: false, startedMinutesAgo: 60 },
  { id: DEMO_SESSION_ID, status: `running`, busy: false, startedMinutesAgo: 52, asks: true },
  { id: DEMO_CHAT_IDS.groupByAssignee, status: `running`, busy: true, startedMinutesAgo: 25 },
  { id: DEMO_CHAT_IDS.sortReports, status: `idle`, busy: false, startedMinutesAgo: DEMO_SERVER_AWAY_MINUTES + 12, away: true },
  { id: DEMO_CHAT_IDS.offlineSpike, status: `running`, busy: false, startedMinutesAgo: 40 },
]

/** A capture that opts out of the closing ask (SCREENSHOT_FEED_NO_QUESTION,
 *  or EXP-1501's SCREENSHOT_FEED_WRITING, which ends on a running turn) has
 *  nothing pending, so the row is not waiting either. */
export function demoNeedsInput(spec: LiveSessionSpec): boolean {
  if (spec.away) return true
  return (
    spec.asks === true &&
    process.env.SCREENSHOT_FEED_NO_QUESTION !== `1` &&
    process.env.SCREENSHOT_FEED_WRITING !== `1`
  )
}

/** The `sessions` columns a live row is written with at `now`. */
export function demoLiveRow(spec: LiveSessionSpec, now: number = Date.now()) {
  return {
    status: spec.status,
    busy: spec.busy,
    needsInput: demoNeedsInput(spec),
    blocked: (spec.away
      ? {
          kind: `device_offline`,
          agent: null,
          window: null,
          resetsAt: null,
          since: new Date(now - DEMO_AWAY_FLAGGED_MINUTES_AGO * 60_000).toISOString(),
          resume: true,
          resumeSince: null,
        }
      : null) as CodingSessionBlocked | null,
    startedAt: new Date(now - spec.startedMinutesAgo * 60_000),
    // An away row's last turn is its device's last beat; nothing ran since.
    lastTurnAt: new Date(spec.away ? now - DEMO_SERVER_AWAY_MINUTES * 60_000 : now),
    updatedAt: new Date(now),
  }
}
