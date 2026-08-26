/**
 * Resolve the seeded demo instance's real ids (EXP-566/EXP-627).
 *
 * The view catalog names its targets by PLACEHOLDER — `issue:$APP-5`,
 * `support:$thread`, `$emptyBoard`, `/support/$supportToken` — because none of
 * those ids exist until `seed:screenshots` has run, and every re-seed rotates
 * them (the users are recreated so the user-scoped Electric shapes get fresh
 * identities). This is the ONE lookup that turns the placeholders into ids, and
 * the only thing in the pipeline that talks to the database.
 *
 * `screenshot-ids.ts` is a thin printer over it (the desktop lane shells out to
 * `bun run screenshots:ids` and parses the JSON); `capture-views.ts` imports it
 * directly, lazily, and only when a route it actually wants carries a
 * placeholder — importing it eagerly would drag `@/db/connection` into every
 * browser capture run.
 *
 * Everything OPTIONAL here is optional for a reason, and a consumer that cannot
 * resolve one skips its view with a note rather than photographing a fallback
 * screen under the right filename.
 */
import { and, asc, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { actions, automations, boards, devices, issues, supportThreads, teams } from "@/db/schema"
import { mintSupportToken } from "@/lib/helpdesk/token"
import {
  DEMO_DEVICE_ID,
  EMPTY_BOARD_SLUG,
  SUPPORT_REPORTER_THREAD_TITLE,
  TEAM_SLUG,
} from "../screenshot-demo"

/** The identifiers the catalog's desktop/native drives name by hand. */
export const WANTED_IDENTIFIERS = [`APP-3`, `APP-5`, `APP-14`] as const

const RESEED = `Run \`bun run seed:screenshots\` first.`

export interface DemoIds {
  teamId: string
  boardId: string
  /** The seeded board with nothing on it — the `board-empty` view. */
  emptyBoardId?: string
  /** Keyed by human identifier: `{ "APP-5": "<uuid>" }`. */
  issues: Record<string, string>
  supportThreadId?: string
  /** The thread the reporter magic-link page is captured on. */
  supportReporterThreadId?: string
  /**
   * The reporter's magic link for `supportReporterThreadId` — a CREDENTIAL.
   * Never log it, never write it into a file that gets committed: it grants
   * anonymous read/write on that conversation for as long as
   * `BETTER_AUTH_SECRET` lives. Omitted (not thrown) when the secret is unset.
   */
  supportToken?: string
  actionId?: string
  deviceId?: string
  automationId?: string
}

export async function resolveDemoIds(): Promise<DemoIds> {
  const [team] = await db
    .select({ id: teams.id })
    .from(teams)
    .where(eq(teams.slug, TEAM_SLUG))
    .limit(1)
  if (!team) {
    throw new Error(`no team with slug "${TEAM_SLUG}" — the seed has not run. ${RESEED}`)
  }

  const [board] = await db
    .select({ id: boards.id })
    .from(boards)
    .where(and(eq(boards.teamId, team.id), eq(boards.slug, `mobile-app`)))
    .limit(1)
  if (!board) {
    throw new Error(`team "${TEAM_SLUG}" has no board "mobile-app". ${RESEED}`)
  }

  // The empty board is a nice-to-have: an older seed did not plant one, and a
  // desktop launched with an unresolvable EXP_DEV_BOARD_ID would just open the
  // last-visited board and photograph a FULL list under `board-empty`. So it is
  // reported as absent and the view skips.
  const [emptyBoard] = await db
    .select({ id: boards.id })
    .from(boards)
    .where(and(eq(boards.teamId, team.id), eq(boards.slug, EMPTY_BOARD_SLUG)))
    .limit(1)

  const issueRows = await db
    .select({ id: issues.id, identifier: issues.identifier })
    .from(issues)
    .where(
      and(eq(issues.boardId, board.id), inArray(issues.identifier, [...WANTED_IDENTIFIERS]))
    )
  const byIdentifier: Record<string, string> = {}
  for (const row of issueRows) {
    if (row.identifier) byIdentifier[row.identifier] = row.id
  }
  const missing = WANTED_IDENTIFIERS.filter((id) => !byIdentifier[id])
  if (missing.length > 0) {
    throw new Error(
      `board "mobile-app" is missing ${missing.join(`, `)} — the seed is stale or partial. ${RESEED}`
    )
  }

  // "any open support thread": the catalog's `support:$thread` is deliberately
  // unpinned, so take the oldest open one for a stable pick across runs.
  const [thread] = await db
    .select({ id: supportThreads.id })
    .from(supportThreads)
    .where(and(eq(supportThreads.teamId, team.id), eq(supportThreads.status, `open`)))
    .orderBy(asc(supportThreads.createdAt))
    .limit(1)
  if (!thread) {
    throw new Error(`team "${TEAM_SLUG}" has no open support thread. ${RESEED}`)
  }

  // The reporter page is captured on ONE named thread, not "the oldest": the
  // view's anchor is that thread's subject, so the two have to agree.
  const [reporterThread] = await db
    .select({ id: supportThreads.id })
    .from(supportThreads)
    .where(
      and(
        eq(supportThreads.teamId, team.id),
        eq(supportThreads.title, SUPPORT_REPORTER_THREAD_TITLE)
      )
    )
    .limit(1)

  // The first saved action by sort order — the seed's "Update dependencies",
  // which is also the row the web action-editor recipe opens.
  const [action] = await db
    .select({ id: actions.id })
    .from(actions)
    .where(eq(actions.teamId, team.id))
    .orderBy(asc(actions.sortOrder))
    .limit(1)
  if (!action) {
    throw new Error(`team "${TEAM_SLUG}" has no saved actions. ${RESEED}`)
  }

  // The demo user's OWN machine, as announced by `screenshots:desktop` — pinned
  // by its steer device id, never "the oldest row". The seed also plants a
  // teammate's shared server, which is deliberately older, and an unfiltered
  // pick would hand the desktop lane a device that is not in the demo user's
  // synced shape at all (Device settings is own-devices-only, so the dialog
  // would silently never open). Absent until the stub has run once, hence no
  // throw.
  const [device] = await db
    .select({ id: devices.id })
    .from(devices)
    .where(eq(devices.deviceId, DEMO_DEVICE_ID))
    .limit(1)

  const [automation] = await db
    .select({ id: automations.id })
    .from(automations)
    .where(eq(automations.teamId, team.id))
    .orderBy(asc(automations.sortOrder))
    .limit(1)

  return {
    teamId: team.id,
    boardId: board.id,
    emptyBoardId: emptyBoard?.id,
    issues: byIdentifier,
    supportThreadId: thread.id,
    supportReporterThreadId: reporterThread?.id,
    // `mintSupportToken` throws without BETTER_AUTH_SECRET; a capture host that
    // has not exported it should skip the reporter view, not fail the run.
    supportToken:
      reporterThread && process.env.BETTER_AUTH_SECRET
        ? mintSupportToken(reporterThread.id)
        : undefined,
    actionId: action.id,
    deviceId: device?.id,
    automationId: automation?.id,
  }
}
