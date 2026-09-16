/**
 * Keep the seeded "X ago" labels where the seed left them (EXP-913).
 *
 * Every client renders these labels against the REAL clock (web date-fns
 * `formatDistanceToNowStrict`, desktop `inbox::relative_time`, iOS
 * `RelativeDateTimeFormatter`, Android `relativeTime`), and none of them rolls
 * over to an absolute date, so neither `SCREENSHOT_FREEZE_NOW` nor a pinned
 * past instant can stabilise them. A five-lane refresh runs for hours after the
 * seed: "1 hour ago" became "4 hours ago" between the first and last lane and
 * rewrote `support-*`, `drafts`, `automations-list` and the `chat` history for
 * nothing.
 *
 * So the rows those views print are SHIFTED forward by however much time has
 * passed since the seed, which keeps every age exactly what the seed wrote. The
 * elapsed time is read off one anchor the seed stamps at a known offset (the
 * nightly automated run's `ended_at`) and which only this shift moves, so the
 * call is idempotent and cheap to repeat: the stand-in desktop runs it on every
 * heartbeat and `capture-views` before every view.
 *
 * `issue_drafts.updated_at` is trigger-stamped to now() on every UPDATE, so the
 * shift runs with `session_replication_role = replica` (no user triggers fire;
 * the dev compose role is a superuser). A role that may not set it still gets
 * everything but the drafts.
 */
import { and, eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  codingSessions,
  issueDrafts,
  supportMessages,
  supportThreads,
  teams,
  users,
} from "@/db/schema"
import { DEMO_EMAIL, DEMO_SESSION_IDS, TEAM_SLUG } from "../screenshot-demo"

/** The seed stamps `ended_at` of this run exactly this long before its clock. */
export const DEMO_CLOCK_ANCHOR = {
  sessionId: DEMO_SESSION_IDS.nightlyTriageRun,
  endedHoursAgo: 8,
} as const

/** The seeded ENDED runs whose started/ended labels are photographed. */
const ENDED_SESSION_IDS = [
  DEMO_SESSION_IDS.pastChat,
  DEMO_SESSION_IDS.nightlyTriageRun,
  DEMO_SESSION_IDS.updateDepsRun,
]

/** Below this the labels cannot have moved; skip the writes (and the sync churn). */
const MIN_SHIFT_MS = 5_000

/**
 * Shift the demo team's labelled rows by the time elapsed since the seed.
 * Returns the shift applied in ms (0 = nothing seeded, or nothing to do).
 */
export async function reclockDemoRows(now: number = Date.now()): Promise<number> {
  const [anchor] = await db
    .select({ endedAt: codingSessions.endedAt, teamId: codingSessions.teamId })
    .from(codingSessions)
    .innerJoin(teams, eq(teams.id, codingSessions.teamId))
    .where(
      and(
        eq(codingSessions.id, DEMO_CLOCK_ANCHOR.sessionId),
        eq(teams.slug, TEAM_SLUG)
      )
    )
    .limit(1)
  if (!anchor?.endedAt) return 0
  const shiftMs =
    now - DEMO_CLOCK_ANCHOR.endedHoursAgo * 3_600_000 - anchor.endedAt.getTime()
  if (shiftMs < MIN_SHIFT_MS) return 0
  const shift = sql`make_interval(secs => ${shiftMs / 1000})`
  const teamId = anchor.teamId

  const shiftRows = async (withDrafts: boolean) =>
    db.transaction(async (tx) => {
      if (withDrafts) await tx.execute(sql`set local session_replication_role = replica`)
      await tx
        .update(codingSessions)
        .set({
          startedAt: sql`${codingSessions.startedAt} + ${shift}`,
          endedAt: sql`${codingSessions.endedAt} + ${shift}`,
          createdAt: sql`${codingSessions.createdAt} + ${shift}`,
        })
        .where(inArray(codingSessions.id, ENDED_SESSION_IDS))
      await tx
        .update(supportThreads)
        .set({
          createdAt: sql`${supportThreads.createdAt} + ${shift}`,
          updatedAt: sql`${supportThreads.updatedAt} + ${shift}`,
          lastReporterSeenAt: sql`${supportThreads.lastReporterSeenAt} + ${shift}`,
        })
        .where(eq(supportThreads.teamId, teamId))
      await tx
        .update(supportMessages)
        .set({
          createdAt: sql`${supportMessages.createdAt} + ${shift}`,
          updatedAt: sql`${supportMessages.updatedAt} + ${shift}`,
        })
        .where(
          inArray(
            supportMessages.threadId,
            tx
              .select({ id: supportThreads.id })
              .from(supportThreads)
              .where(eq(supportThreads.teamId, teamId))
          )
        )
      if (!withDrafts) return
      await tx
        .update(issueDrafts)
        .set({
          createdAt: sql`${issueDrafts.createdAt} + ${shift}`,
          updatedAt: sql`${issueDrafts.updatedAt} + ${shift}`,
        })
        .where(
          and(
            eq(issueDrafts.teamId, teamId),
            inArray(
              issueDrafts.userId,
              tx.select({ id: users.id }).from(users).where(eq(users.email, DEMO_EMAIL))
            )
          )
        )
    })

  try {
    await shiftRows(true)
  } catch {
    // Not allowed to set the replication role: shift what needs no bypass.
    await shiftRows(false)
  }
  return shiftMs
}
