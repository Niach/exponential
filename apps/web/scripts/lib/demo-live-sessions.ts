/**
 * The seeded LIVE coding sessions, as one re-assertable spec (EXP-913).
 *
 * Three rows are live on purpose: the demo user's running claude showcase on
 * APP-5 (the `steering` view), Mira's running row on APP-4 and the demo user's
 * in_review codex row on APP-14. A live row is exactly what the server's
 * staleness sweep reaps (lib/coding-session-sweep.ts): `running`/`in_review`
 * with `updated_at` older than contract `codingSession.staleHours` (2h), and
 * since no seeded device advertises `stale-end` it DELETES them. A full
 * five-lane refresh runs for hours; the stand-in desktop only heartbeated the
 * demo user's rows while it was up, and Mira's never, so the rows vanished
 * mid-wave and a rerun's stub died on "no running coding session for demo@".
 *
 * So the seed writes these rows through `assertDemoLiveSessions`, and the stub
 * (screenshot-desktop.ts) calls it again at boot and on every heartbeat: a row
 * the sweep already took comes back under its pinned id, a row a capture ended
 * goes back to its seeded status, and `started_at` is re-anchored to the
 * current clock so "started 1h ago" reads the same on the last lane as on the
 * first.
 */
import { and, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, codingSessions, issues, teams } from "@/db/schema"
import {
  DEMO_DEVICE_ID,
  DEMO_DEVICE_LABEL,
  DEMO_SESSION_IDS,
  DEMO_STEERED_SESSION_ID,
  DEMO_USER_ID,
  TEAM_SLUG,
} from "../screenshot-demo"

/** The board the three live rows' issues sit on (`APP-*`). */
const BOARD_SLUG = `mobile-app`

interface LiveSessionSpec {
  id: string
  identifier: string
  userId: string
  status: `running` | `in_review`
  startedMinutesAgo: number
  deviceId: string | null
  deviceLabel: string
  agent: `claude` | `codex` | null
}

export const DEMO_LIVE_SESSIONS: readonly LiveSessionSpec[] = [
  // EXP-733: the demo user's own rows name the MACHINE and the AGENT the way a
  // real launch stamps them — `sessionAgentUsage` (×4) joins on both.
  {
    id: DEMO_STEERED_SESSION_ID,
    identifier: `APP-5`,
    userId: DEMO_USER_ID,
    status: `running`,
    startedMinutesAgo: 60,
    deviceId: DEMO_DEVICE_ID,
    deviceLabel: DEMO_DEVICE_LABEL,
    agent: `claude`,
  },
  // Mira's row stays bare: her machine never registers.
  {
    id: DEMO_SESSION_IDS.miraRunning,
    identifier: `APP-4`,
    userId: `demo-mira`,
    status: `running`,
    startedMinutesAgo: 20,
    deviceId: null,
    deviceLabel: `Mira's Mac mini`,
    agent: null,
  },
  {
    id: DEMO_SESSION_IDS.reviewCodex,
    identifier: `APP-14`,
    userId: DEMO_USER_ID,
    status: `in_review`,
    startedMinutesAgo: 180,
    deviceId: DEMO_DEVICE_ID,
    deviceLabel: DEMO_DEVICE_LABEL,
    agent: `codex`,
  },
]

/**
 * Upsert the live rows against the seeded team. Returns how many were written;
 * 0 means the demo team (or its issues) is not seeded, which the caller
 * reports rather than this module guessing at a fix.
 */
export async function assertDemoLiveSessions(now: number = Date.now()): Promise<number> {
  const [team] = await db
    .select({ id: teams.id })
    .from(teams)
    .where(eq(teams.slug, TEAM_SLUG))
    .limit(1)
  if (!team) return 0
  const rows = await db
    .select({ id: issues.id, identifier: issues.identifier })
    .from(issues)
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .where(
      and(
        eq(boards.teamId, team.id),
        eq(boards.slug, BOARD_SLUG),
        inArray(
          issues.identifier,
          DEMO_LIVE_SESSIONS.map((spec) => spec.identifier)
        )
      )
    )
  const issueIds = new Map(rows.map((row) => [row.identifier, row.id]))

  let written = 0
  for (const spec of DEMO_LIVE_SESSIONS) {
    const issueId = issueIds.get(spec.identifier)
    if (!issueId) continue
    const values = {
      issueId,
      teamId: team.id,
      userId: spec.userId,
      deviceId: spec.deviceId,
      deviceLabel: spec.deviceLabel,
      agent: spec.agent,
      status: spec.status,
      startedAt: new Date(now - spec.startedMinutesAgo * 60_000),
      endedAt: null,
      endedBy: null,
      updatedAt: new Date(now),
    }
    await db
      .insert(codingSessions)
      .values({ id: spec.id, ...values })
      .onConflictDoUpdate({ target: codingSessions.id, set: values })
    written += 1
  }
  return written
}
