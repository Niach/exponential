import { and, desc, eq, inArray, isNull, type SQL } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { UUID_RE } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { issues } from "@/db/schema"
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
  const conditions: SQL[] = [
    inArray(issues.teamId, teamIds),
    isNull(issues.boardDeletedAt),
    isNull(issues.boardArchivedAt),
    eq(issues.identifier, idOrIdentifier.toUpperCase()),
  ]
  if (opts?.grantedBoardIds) {
    if (opts.grantedBoardIds.length === 0) throw notFound()
    conditions.push(inArray(issues.boardId, opts.grantedBoardIds))
  }
  const [row] = await db
    .select({ id: issues.id })
    .from(issues)
    .where(and(...conditions))
    .orderBy(desc(issues.createdAt))
    .limit(1)
  if (!row) throw notFound()
  return row.id
}
