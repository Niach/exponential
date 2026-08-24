/**
 * Print the seeded demo instance's real UUIDs as JSON (EXP-566).
 *
 * The view catalog names desktop/native targets by placeholder — `issue:$APP-5`,
 * `support:$thread` — precisely because the ids only exist after a seed run and
 * rotate on every re-seed (seed-screenshots.ts recreates the users so the
 * user-scoped Electric shapes get fresh identities). The native and desktop
 * capture lanes resolve those placeholders against this output.
 *
 * Usage (from apps/web, after `bun run seed:screenshots`):
 *   bun run screenshots:ids
 *
 * Emits, on stdout, exactly:
 *   { teamId, boardId, issues: { "APP-3": …, "APP-5": …, "APP-14": … },
 *     supportThreadId, actionId }
 */
import { and, asc, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { actions, boards, issues, supportThreads, teams } from "@/db/schema"
import { TEAM_SLUG } from "./screenshot-demo"

/** The identifiers the catalog's desktop/native drives name by hand. */
const WANTED_IDENTIFIERS = [`APP-3`, `APP-5`, `APP-14`] as const

const RESEED = `Run \`bun run seed:screenshots\` first.`

async function main() {
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

  const issueRows = await db
    .select({ id: issues.id, identifier: issues.identifier })
    .from(issues)
    .where(
      and(
        eq(issues.boardId, board.id),
        inArray(issues.identifier, [...WANTED_IDENTIFIERS])
      )
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

  console.log(
    JSON.stringify(
      {
        teamId: team.id,
        boardId: board.id,
        issues: byIdentifier,
        supportThreadId: thread.id,
        actionId: action.id,
      },
      null,
      2
    )
  )
  process.exit(0)
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err)
  process.exit(1)
})
