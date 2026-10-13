/**
 * The seeded LIVE sessions, as one re-assertable spec (EXP-913, EXP-1293).
 *
 * Five `sessions` rows are live on purpose: the cold-start thread's root and
 * its child "Profile the launch path" (the `thread`, `session` and `steering`
 * views; the child waits on a question), the APP-14 thread (busy), the FB-9
 * thread the Triage agent started (busy) and the private offline-queue spike.
 * A full five-lane refresh runs for hours, and a capture may stop or answer a
 * session, so the seed writes their state through this spec and the stub
 * (screenshot-desktop.ts) re-asserts it at boot and on every heartbeat: a row
 * a capture moved goes back to its seeded status, and `started_at` is
 * re-anchored to the current clock so "1h 00m" reads the same on the last lane
 * as on the first.
 *
 * TEMP until EXP-1291's server half moves `steer.mintTicket` to `sessions`:
 * the viewer ticket for the raw session view still validates against
 * `coding_sessions`, so ONE legacy row under DEMO_SESSION_ID is upserted here
 * too — re-asserted because the coding-session staleness sweep deletes a
 * `running` row nobody heartbeats.
 */
import { and, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, codingSessions, issues, sessions, teams } from "@/db/schema"
import {
  DEMO_DEVICE_ID,
  DEMO_DEVICE_LABEL,
  DEMO_SESSION_ID,
  DEMO_THREAD_IDS,
  DEMO_USER_ID,
  TEAM_SLUG,
} from "../screenshot-demo"

export interface LiveSessionSpec {
  id: string
  status: `running` | `idle`
  /** Device-written turn state: the list spinner. */
  busy: boolean
  startedMinutesAgo: number
  /** The stub's feed for this row ends on an unanswered question
   *  (screenshot-desktop.ts) and the thread carries the question card, so
   *  the row says so the way a real device stamps it: `needs_input`. */
  asks?: boolean
}

export const DEMO_LIVE_SESSIONS: readonly LiveSessionSpec[] = [
  { id: DEMO_THREAD_IDS.coldStart, status: `running`, busy: false, startedMinutesAgo: 60 },
  { id: DEMO_SESSION_ID, status: `running`, busy: false, startedMinutesAgo: 52, asks: true },
  { id: DEMO_THREAD_IDS.groupByAssignee, status: `running`, busy: true, startedMinutesAgo: 25 },
  { id: DEMO_THREAD_IDS.sortReports, status: `running`, busy: true, startedMinutesAgo: 8 },
  { id: DEMO_THREAD_IDS.offlineSpike, status: `running`, busy: false, startedMinutesAgo: 40 },
]

/** A capture that opts out of the closing ask (SCREENSHOT_FEED_NO_QUESTION)
 *  has nothing pending, so the row is not waiting either. */
export function demoNeedsInput(spec: LiveSessionSpec): boolean {
  return spec.asks === true && process.env.SCREENSHOT_FEED_NO_QUESTION !== `1`
}

/**
 * Put the live rows back to their seeded state. Returns how many `sessions`
 * rows it found; 0 means the demo team is not seeded, which the caller
 * reports rather than this module guessing at a fix.
 */
export async function assertDemoLiveSessions(now: number = Date.now()): Promise<number> {
  const [team] = await db
    .select({ id: teams.id })
    .from(teams)
    .where(eq(teams.slug, TEAM_SLUG))
    .limit(1)
  if (!team) return 0

  let written = 0
  for (const spec of DEMO_LIVE_SESSIONS) {
    const rows = await db
      .update(sessions)
      .set({
        status: spec.status,
        busy: spec.busy,
        needsInput: demoNeedsInput(spec),
        startedAt: new Date(now - spec.startedMinutesAgo * 60_000),
        endedAt: null,
        lastTurnAt: new Date(now),
        updatedAt: new Date(now),
      })
      .where(and(eq(sessions.id, spec.id), eq(sessions.teamId, team.id)))
      .returning({ id: sessions.id })
    written += rows.length
  }

  await assertSteerTicketRow(team.id, now)
  return written
}

/**
 * TEMP until EXP-1291's server half moves mintTicket to sessions: the one
 * legacy `coding_sessions` row `steer.mintTicket` validates the raw session
 * view's viewer ticket against (owner = the demo user, on APP-5).
 */
async function assertSteerTicketRow(teamId: string, now: number): Promise<void> {
  const [issue] = await db
    .select({ id: issues.id })
    .from(issues)
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .where(
      and(
        eq(boards.teamId, teamId),
        eq(boards.slug, `mobile-app`),
        inArray(issues.identifier, [`APP-5`])
      )
    )
    .limit(1)
  if (!issue) return
  const spec = DEMO_LIVE_SESSIONS.find((entry) => entry.id === DEMO_SESSION_ID)!
  const values = {
    issueId: issue.id,
    teamId,
    userId: DEMO_USER_ID,
    deviceId: DEMO_DEVICE_ID,
    deviceLabel: DEMO_DEVICE_LABEL,
    agent: `claude`,
    status: `running` as const,
    needsInput: demoNeedsInput(spec),
    startedAt: new Date(now - spec.startedMinutesAgo * 60_000),
    endedAt: null,
    endedBy: null,
    updatedAt: new Date(now),
  }
  // TEMP until EXP-1291's server half moves mintTicket to sessions
  await db
    .insert(codingSessions)
    .values({ id: DEMO_SESSION_ID, ...values })
    .onConflictDoUpdate({ target: codingSessions.id, set: values })
}
