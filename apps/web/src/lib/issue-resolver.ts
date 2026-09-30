import { and, desc, eq, inArray, isNull, sql, type SQL } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { UUID_RE } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { issueEvents, issues } from "@/db/schema"
import { getUserTeamIds } from "@/lib/team-membership"

// EXP-707: the ONE issue-reference resolver — a UUID passes through, a human
// identifier ("EXP-42") resolves to its issue UUID, scoped to the caller's
// teams and excluding trashed/archived boards (the trigger-maintained
// board mirrors). Identifier collisions are possible — across teams AND
// within one (nothing enforces per-team prefix uniqueness; boards' only
// composite unique is (team_id, slug)) — so the newest match wins,
// DETERMINISTICALLY, on every surface (the old MCP copy had no orderBy).
//
// `grantedBoardIds` is the MCP OAuth confinement: when present the lookup is
// additionally restricted to those boards, so a confined token can never
// resolve an identifier it wasn't granted. The team-level access check still
// runs in the caller — this only maps the friendly identifier to the row id.
//
// FEED-66: a RETIRED identifier resolves too. A cross-board move renumbers
// the issue (FEED-50 becomes EXP-1147) while a run launched on it keeps
// calling it FEED-50 — `issues_get` answered, `pr_open` minutes later said
// "Issue not found". Every move records the retired identifier in a
// `board_moved` event (EXP-57, the branch-parse fallback in pr-sync reads the
// same), so the second chance is that event, scoped exactly like the direct
// lookup on the issue's CURRENT board. Identifiers are monotonic and never
// reused, so within a team a retired one names exactly one issue; the newest
// move wins across teams, deterministically like above.
export async function resolveIssueReference(
  userId: string,
  idOrIdentifier: string,
  opts?: { grantedBoardIds?: string[] }
): Promise<string> {
  if (UUID_RE.test(idOrIdentifier)) return idOrIdentifier
  const notFound = () =>
    new TRPCError({
      code: `NOT_FOUND`,
      message: `Issue not found: ${idOrIdentifier}`,
    })
  const teamIds = await getUserTeamIds(userId)
  if (teamIds.length === 0) throw notFound()
  const identifier = idOrIdentifier.toUpperCase()
  const scope: SQL[] = [
    inArray(issues.teamId, teamIds),
    isNull(issues.boardDeletedAt),
    isNull(issues.boardArchivedAt),
  ]
  if (opts?.grantedBoardIds) {
    if (opts.grantedBoardIds.length === 0) throw notFound()
    scope.push(inArray(issues.boardId, opts.grantedBoardIds))
  }
  const [row] = await db
    .select({ id: issues.id })
    .from(issues)
    .where(and(...scope, eq(issues.identifier, identifier)))
    .orderBy(desc(issues.createdAt))
    .limit(1)
  if (row) return row.id

  const [moved] = await db
    .select({ id: issues.id })
    .from(issueEvents)
    .innerJoin(issues, eq(issues.id, issueEvents.issueId))
    .where(
      and(
        ...scope,
        eq(issueEvents.type, `board_moved`),
        sql`${issueEvents.payload} ->> 'fromIdentifier' = ${identifier}`
      )
    )
    .orderBy(desc(issueEvents.createdAt))
    .limit(1)
  if (!moved) throw notFound()
  return moved.id
}

// FEED-66: the identifiers an issue carried BEFORE its cross-board moves,
// newest move first — what a branch pushed under the old name is called
// (`exp/FEED-50` for what is now EXP-1147). Empty for a never-moved issue.
export async function retiredIdentifiers(issueId: string): Promise<string[]> {
  const rows = await db
    .select({
      identifier: sql<string | null>`${issueEvents.payload} ->> 'fromIdentifier'`,
    })
    .from(issueEvents)
    .where(
      and(eq(issueEvents.issueId, issueId), eq(issueEvents.type, `board_moved`))
    )
    .orderBy(desc(issueEvents.createdAt))
  const seen = new Set<string>()
  const retired: string[] = []
  for (const row of rows) {
    const identifier =
      typeof row.identifier === `string` ? row.identifier.trim() : ``
    if (!identifier || seen.has(identifier)) continue
    seen.add(identifier)
    retired.push(identifier)
  }
  return retired
}
