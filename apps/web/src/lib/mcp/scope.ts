import {
  and,
  eq,
  gt,
  inArray,
  isNull,
  or,
  type Column,
  type SQL,
} from "drizzle-orm"
import { db } from "@/db/connection"
import { mcpGrants, oauthAccessTokens, boards } from "@/db/schema"
import { boardVisible } from "@/lib/board-visibility"

// What an /api/mcp request may touch. Session-cookie and personal `expu_`
// api-key requests are the user's own credentials and get `full` access; an
// OAuth access token (a human MCP client like Claude) is confined to the
// teams/boards its consent grant selected. A token whose (user,
// client) pair has no grant row gets NOTHING — the holder must re-run the
// consent flow. The scope is enforced in the MCP tool layer; OAuth tokens are
// not accepted anywhere else (see resolveSession), so the tool layer is the
// complete surface.
export interface McpAccess {
  full: boolean
  /** Whole-team grants — includes boards created later. */
  fullTeamIds: ReadonlySet<string>
  /** Individually granted boards. */
  grantedBoardIds: ReadonlySet<string>
  /**
   * Teams reachable at all: fully granted ones plus the hosts of
   * individually granted boards (those need team-level aux reads —
   * labels, members — for issue workflows).
   */
  visibleTeamIds: ReadonlySet<string>
}

export const FULL_ACCESS: McpAccess = {
  full: true,
  fullTeamIds: new Set(),
  grantedBoardIds: new Set(),
  visibleTeamIds: new Set(),
}

export const NO_ACCESS: McpAccess = {
  full: false,
  fullTeamIds: new Set(),
  grantedBoardIds: new Set(),
  visibleTeamIds: new Set(),
}

interface GrantShape {
  allTeams: boolean
  teamIds: string[]
  boardIds: string[]
}

// Pure core, unit-testable: `boardTeamIds` maps each granted board
// to its team id (resolved by the caller).
export function buildMcpAccess(
  grant: GrantShape,
  boardTeamIds: ReadonlyMap<string, string>
): McpAccess {
  if (grant.allTeams) return FULL_ACCESS
  const fullTeamIds = new Set(grant.teamIds)
  const grantedBoardIds = new Set(grant.boardIds)
  const visibleTeamIds = new Set(grant.teamIds)
  for (const boardId of grant.boardIds) {
    const teamId = boardTeamIds.get(boardId)
    if (teamId) visibleTeamIds.add(teamId)
  }
  return { full: false, fullTeamIds, grantedBoardIds, visibleTeamIds }
}

export async function resolveMcpAccessForGrant(
  grant: GrantShape | null | undefined
): Promise<McpAccess> {
  if (!grant) return NO_ACCESS
  if (grant.allTeams) return FULL_ACCESS
  const map = new Map<string, string>()
  if (grant.boardIds.length > 0) {
    // Trash-aware: a granted board sitting in the 48h trash must not keep
    // its host team visible for team-level aux reads.
    const rows = await db
      .select({ id: boards.id, teamId: boards.teamId })
      .from(boards)
      .where(
        and(inArray(boards.id, grant.boardIds), boardVisible())
      )
    for (const row of rows) map.set(row.id, row.teamId)
  }
  return buildMcpAccess(grant, map)
}

// Resolve an OAuth access token (already stripped of "Bearer ") to its user +
// scope. Returns null for unknown or expired tokens. Unlike better-auth's
// getMcpSession this DOES enforce access-token expiry — clients hold a
// refresh token and recover from the resulting 401.
export async function resolveMcpTokenAccess(
  bearerToken: string
): Promise<{ userId: string; clientId: string; access: McpAccess } | null> {
  const [token] = await db
    .select({
      userId: oauthAccessTokens.userId,
      clientId: oauthAccessTokens.clientId,
      expiresAt: oauthAccessTokens.accessTokenExpiresAt,
    })
    .from(oauthAccessTokens)
    .where(
      and(
        eq(oauthAccessTokens.accessToken, bearerToken),
        gt(oauthAccessTokens.accessTokenExpiresAt, new Date())
      )
    )
    .limit(1)
  if (!token?.userId) return null

  const [grant] = await db
    .select({
      allTeams: mcpGrants.allTeams,
      teamIds: mcpGrants.teamIds,
      boardIds: mcpGrants.boardIds,
    })
    .from(mcpGrants)
    .where(
      and(
        eq(mcpGrants.userId, token.userId),
        eq(mcpGrants.clientId, token.clientId)
      )
    )
    .limit(1)

  return {
    userId: token.userId,
    clientId: token.clientId,
    access: await resolveMcpAccessForGrant(grant),
  }
}

const deniedMessage = (target: string) =>
  `This MCP connection was not granted access to ${target}. Re-authenticate the MCP server and adjust the team/board selection on the consent screen.`

export function isTeamVisible(access: McpAccess, teamId: string) {
  return access.full || access.visibleTeamIds.has(teamId)
}

export function isTeamFullyGranted(
  access: McpAccess,
  teamId: string
) {
  return access.full || access.fullTeamIds.has(teamId)
}

export function isBoardGranted(
  access: McpAccess,
  boardId: string,
  teamId: string
) {
  return (
    access.full ||
    access.grantedBoardIds.has(boardId) ||
    access.fullTeamIds.has(teamId)
  )
}

/** Team-level reads needed by issue workflows: labels, members, repos. */
export function assertTeamVisible(access: McpAccess, teamId: string) {
  if (!isTeamVisible(access, teamId)) {
    throw new Error(deniedMessage(`this team`))
  }
}

/** Team-level mutations: settings, invites, labels, repos, new boards. */
export function assertTeamFullyGranted(
  access: McpAccess,
  teamId: string
) {
  if (!isTeamFullyGranted(access, teamId)) {
    throw new Error(
      deniedMessage(`team-level operations in this team`)
    )
  }
}

export function assertBoardGranted(
  access: McpAccess,
  boardId: string,
  teamId: string
) {
  if (!isBoardGranted(access, boardId, teamId)) {
    throw new Error(deniedMessage(`this board`))
  }
}

/** Operations spanning every team (create team, whole inbox). */
export function assertFullAccess(access: McpAccess) {
  if (!access.full) {
    throw new Error(
      deniedMessage(`data outside its granted teams/boards`)
    )
  }
}

export function filterVisibleTeamIds(
  access: McpAccess,
  teamIds: string[]
): string[] {
  if (access.full) return teamIds
  return teamIds.filter((id) => access.visibleTeamIds.has(id))
}

// ── Row scoping (EXP-639) ────────────────────────────────────────────────────
// ONE encoding of "which rows may this connection touch", in the two forms the
// tools need: a SQL filter for list queries and its row-level twin for
// single-row reads. Both answer the same three questions — is the row's board
// granted, is its whole team granted, or is it a BOARD-LESS row of the
// caller's own inside a visible team. That last arm exists because batch,
// action and chat runs carry `board_id` NULL: without it a board-confined
// grant could start a batch run it could never read back, list or kill.

/** `grantScopeFilter` found no clause at all: the grant reaches nothing, so
 *  the caller returns an empty list WITHOUT querying. */
export const GRANT_MATCHES_NOTHING = false

/** A grant that selected no board and no team reaches nothing at all — the
 *  board-less arm below EXTENDS a grant, it never stands in for one. */
function grantsNothing(access: McpAccess): boolean {
  return (
    !access.full &&
    access.grantedBoardIds.size === 0 &&
    access.fullTeamIds.size === 0
  )
}

export interface GrantScopeColumns {
  /** The row's board — NULLABLE on coding_sessions (issue-less runs). */
  boardCol: Column
  /** The row's team. */
  teamCol: Column
  /**
   * Owner columns (`user_id` / `host_user_id`). Passed WITH `userId` they
   * admit the caller's own board-less rows inside a visible team; omitted,
   * board-less rows need a whole-team grant (the notifications encoding,
   * whose join already drops issue-less rows).
   */
  ownerCols?: readonly Column[]
  userId?: string
}

/**
 * The grant as a WHERE fragment. `undefined` = unrestricted (full access);
 * `GRANT_MATCHES_NOTHING` = the grant covers nothing, so skip the query.
 * Filtering in SQL is deliberate: a post-limit JS filter under-fills pages and
 * makes offset pagination skip in-scope rows.
 */
export function grantScopeFilter(
  access: McpAccess,
  cols: GrantScopeColumns
): SQL | undefined | typeof GRANT_MATCHES_NOTHING {
  if (access.full) return undefined
  if (grantsNothing(access)) return GRANT_MATCHES_NOTHING
  const clauses: SQL[] = []
  const grantedBoardIds = [...access.grantedBoardIds]
  if (grantedBoardIds.length > 0) {
    clauses.push(inArray(cols.boardCol, grantedBoardIds))
  }
  const fullTeamIds = [...access.fullTeamIds]
  if (fullTeamIds.length > 0) {
    clauses.push(inArray(cols.teamCol, fullTeamIds))
  }
  const visibleTeamIds = [...access.visibleTeamIds]
  const { ownerCols, userId } = cols
  if (ownerCols?.length && userId && visibleTeamIds.length > 0) {
    clauses.push(
      and(
        isNull(cols.boardCol),
        inArray(cols.teamCol, visibleTeamIds),
        or(...ownerCols.map((col) => eq(col, userId)))!
      )!
    )
  }
  if (clauses.length === 0) return GRANT_MATCHES_NOTHING
  return or(...clauses)!
}

export interface GrantScopeRow {
  teamId: string | null
  boardId: string | null
  userId?: string | null
  hostUserId?: string | null
}

/** The row-level twin of `grantScopeFilter` — same three arms, same order. */
export function isRowGranted(
  access: McpAccess,
  row: GrantScopeRow,
  callerUserId?: string
): boolean {
  if (access.full) return true
  if (grantsNothing(access)) return false
  if (!row.teamId) return false
  if (row.boardId) return isBoardGranted(access, row.boardId, row.teamId)
  if (isTeamFullyGranted(access, row.teamId)) return true
  if (!callerUserId) return false
  const own =
    row.userId === callerUserId || row.hostUserId === callerUserId
  return own && isTeamVisible(access, row.teamId)
}
