// EXP-1490: who sees and calls which app tool. Pure and byte-locked by
// `fixtures/access.json` (its `$comment` is the spec): the grant model of
// `mcp_grants`, the member × app policy and the access rule.

import fixture from "../fixtures/access.json" with { type: "json" }
import type { ToolAccess } from "./exposes"

export const GRANT_KINDS = fixture.grantKinds as readonly GrantKind[]
export const GRANT_ACCESS = fixture.grantAccess as readonly GrantAccess[]
export const POLICY_LEVELS = fixture.levels as readonly Level[]

export type GrantKind = `builtin` | `apps`
/** The consent page's Read only | Full. */
export type GrantAccess = `read` | `full`
export type Level = `none` | `read` | `write`

/** The `mcp_grants` columns this contract reads. */
export interface Grant {
  kind: GrantKind
  allTeams: boolean
  teamIds: readonly string[]
  boardIds: readonly string[]
  allApps: boolean
  appIds: readonly string[]
  access: GrantAccess
}

const RANK: Record<Level, number> = { none: 0, read: 1, write: 2 }

function lower(a: Level, b: Level): Level {
  return RANK[a] <= RANK[b] ? a : b
}

/** A grant's access as a level (`full` = `write`). */
export function grantLevel(access: GrantAccess): Level {
  return access === `full` ? `write` : `read`
}

/** Every reason a grant row is malformed (empty = valid). */
export function grantErrors(grant: Grant): string[] {
  const errors: string[] = []
  if (!GRANT_KINDS.includes(grant.kind)) errors.push(`kind`)
  if (!GRANT_ACCESS.includes(grant.access)) errors.push(`access`)
  if (grant.kind === `apps`) {
    if (grant.allTeams || grant.teamIds.length !== 1) errors.push(`team`)
    if (grant.boardIds.length > 0) errors.push(`boards`)
    if (grant.allApps === grant.appIds.length > 0) errors.push(`apps`)
  } else if (grant.kind === `builtin`) {
    if (grant.allApps || grant.appIds.length > 0) errors.push(`apps`)
    if (grant.access !== `full`) errors.push(`access`)
  }
  return errors
}

/** #FEED-242: what decides a member's level without a row: the app's
 *  default (`apps.default_access`) and whether the member is exempt from
 *  it (the app's creator or a team owner). */
export interface PolicyContext {
  default?: Level | null
  exempt?: boolean
}

/** The member's policy for an app (the Settings › MCP Access table): the
 *  row, else `write` for an exempt member, else the app's default (no
 *  context = `write`). */
export function memberPolicy(row: Level | null | undefined, context: PolicyContext = {}): Level {
  if (row) return row
  if (context.exempt) return `write`
  return context.default ?? `write`
}

export interface AccessInput {
  grant: Grant
  /** The app's id and team. */
  appId: string
  teamId: string
  /** The app is trashed. */
  appDeleted: boolean
  /** The caller's role in the app's team, null = not a member. */
  role: string | null
  /** The caller's policy row for this app, null = none. */
  policy: Level | null
}

/** Effective access to one app = grant ∩ the member's policy; `none` for
 *  an app the grant does not cover, a trashed app, a non-member or a
 *  malformed grant. */
export function effectiveAccess(input: AccessInput): Level {
  const { grant } = input
  if (grant.kind !== `apps` || grantErrors(grant).length > 0) return `none`
  if (input.role === null || input.appDeleted) return `none`
  if (grant.teamIds[0] !== input.teamId) return `none`
  if (!grant.allApps && !grant.appIds.includes(input.appId)) return `none`
  return lower(grantLevel(grant.access), memberPolicy(input.policy))
}

/** May a caller with `level` see (and call) a tool of `access`? A read
 *  token never sees a write tool. */
export function toolVisible(level: Level, access: ToolAccess): boolean {
  return RANK[level] >= RANK[access]
}

export interface AssistantScopeInput {
  /** The member's hidden personal team (the Assistant's own conversation). */
  personalTeamId: string
  /** The teams the member belongs to now, in their order. */
  memberTeamIds: readonly string[]
  /** The composer's Teams pick on the conversation root; null = every team. */
  pickedTeamIds: readonly string[] | null
}

/** #EXP-1720: the teams an Assistant tree may touch this turn: the
 *  personal team, then the picked teams ∩ the member's (null = all). */
export function assistantScope(input: AssistantScopeInput): string[] {
  const picked = input.pickedTeamIds === null ? null : new Set(input.pickedTeamIds)
  const teams = input.memberTeamIds.filter((id) => picked === null || picked.has(id))
  return [...new Set([input.personalTeamId, ...teams])]
}
