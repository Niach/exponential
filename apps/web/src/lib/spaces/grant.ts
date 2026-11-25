// #EXP-2161 (domain-contract member-space.json): what a member space may do
// NOW, read from the rows per request, never from its key. Router-free:
// lib/sessions and the watchdog import this.
import { and, eq, inArray, isNotNull, isNull, or, sql, type SQL } from "drizzle-orm"
import { alias } from "drizzle-orm/pg-core"
import { MEMBER_SPACE, spaceEnds, spaceTeams } from "@exp/domain-contract/member-space"
import { agents, devices, installApprovals, sessions, teamMembers, teams, type Session } from "@/db/schema"
import type { SpaceGrant } from "@/lib/spaces/context"

/** Anything that can `select` (the pool, a transaction). */
export type SpaceReader = Pick<typeof import("@/db/connection").db, `select`>

export const SPACE_INSTALL_KIND = MEMBER_SPACE.installKind
export const SPACE_APPROVAL_KIND = MEMBER_SPACE.approvalKind as `space`
export const SPACE_KEY_KIND = MEMBER_SPACE.keyKind as `space`

/** A machine id only `memberSpaces.request` makes. */
export const isSpaceDeviceId = (deviceId: string | null | undefined): boolean =>
  !!deviceId && deviceId.startsWith(MEMBER_SPACE.deviceIdPrefix)

const host = alias(devices, `space_host`)

/** A space row with what decides whether it lives. */
export interface SpaceRow {
  spaceId: string
  deviceId: string
  userId: string
  label: string
  spaceTeamId: string | null
  createdAt: Date
  hostRowId: string
  hostUserId: string
  hostDeviceId: string
  hostLabel: string
  /** `spaceTeams` NOW. */
  teamIds: string[]
  live: boolean
}

/** The space rows `where` names, each with its teams and whether it lives. */
export async function spaceRows(exec: SpaceReader, where: SQL | undefined): Promise<SpaceRow[]> {
  const rows = await exec
    .select({
      spaceId: devices.id,
      deviceId: devices.deviceId,
      userId: devices.userId,
      label: devices.label,
      spaceTeamId: devices.spaceTeamId,
      createdAt: devices.createdAt,
      hostRowId: host.id,
      hostUserId: host.userId,
      hostDeviceId: host.deviceId,
      hostLabel: host.label,
      hostKind: host.kind,
      sharedTeamIds: host.sharedTeamIds,
      assistantTeamIds: host.assistantTeamIds,
    })
    .from(devices)
    .innerJoin(host, eq(host.id, devices.spaceHostId))
    .where(and(isNotNull(devices.spaceHostId), where))
    .orderBy(devices.createdAt, devices.id)
  if (rows.length === 0) return []
  const userIds = [...new Set(rows.flatMap((row) => [row.userId, row.hostUserId]))]
  const members = await exec
    .select({ userId: teamMembers.userId, teamId: teamMembers.teamId })
    .from(teamMembers)
    .where(inArray(teamMembers.userId, userIds))
    .orderBy(teamMembers.createdAt, teamMembers.teamId)
  const teamsOf = (userId: string) => members.filter((row) => row.userId === userId).map((row) => row.teamId)
  return rows.map(({ hostKind, sharedTeamIds, assistantTeamIds, ...row }) => {
    const opted = hostKind === `server` ? (assistantTeamIds ?? []).filter((id) => (sharedTeamIds ?? []).includes(id)) : []
    const teamIds = row.userId === row.hostUserId ? [] : spaceTeams(opted, teamsOf(row.hostUserId), teamsOf(row.userId))
    return { ...row, teamIds, live: row.spaceTeamId !== null && !spaceEnds(row.spaceTeamId, teamIds) }
  })
}

/** What the space `spaceId` of `userId` may reach now; null = it is gone (or
 *  due to end): its keys grant nothing. */
export async function loadSpaceGrant(exec: SpaceReader, userId: string, spaceId: string): Promise<SpaceGrant | null> {
  const [row] = await spaceRows(exec, and(eq(devices.id, spaceId), eq(devices.userId, userId)))
  if (!row?.live) return null
  const [personal] = await exec
    .select({ id: teams.id })
    .from(teams)
    .where(and(eq(teams.ownerId, userId), eq(teams.kind, `personal`)))
    .limit(1)
  const [assistant] = await exec
    .select({ id: agents.id, deviceId: agents.deviceId })
    .from(agents)
    .where(and(eq(agents.ownerId, userId), eq(agents.role, `assistant`)))
    .limit(1)
  const personalTeamId = personal?.id ?? null
  return {
    spaceId: row.spaceId,
    deviceId: row.deviceId,
    userId: row.userId,
    label: row.label,
    spaceTeamId: row.spaceTeamId!,
    hostRowId: row.hostRowId,
    hostUserId: row.hostUserId,
    hostDeviceId: row.hostDeviceId,
    teamIds: row.teamIds,
    personalTeamId,
    reach: [...(personalTeamId ? [personalTeamId] : []), ...row.teamIds],
    assistantId: assistant?.id ?? null,
    createdAt: row.createdAt,
  }
}

/** The teams an Assistant conversation on `deviceId` is narrowed to: null =
 *  not a space (no narrowing); a space that is gone = none. */
export async function spaceTeamsOfDevice(exec: SpaceReader, userId: string, deviceId: string): Promise<string[] | null> {
  if (!isSpaceDeviceId(deviceId)) return null
  const [row] = await spaceRows(exec, and(eq(devices.userId, userId), eq(devices.deviceId, deviceId)))
  return row?.live ? row.teamIds : []
}

// ── sessions ──────────────────────────────────────────────────────────────

/** SQL on `sessions.root_id`: the space's own trees (the roots of the
 *  sessions that name its device). */
export function spaceTreeCondition(grant: Pick<SpaceGrant, `deviceId` | `userId`>, column: SQL = sql`${sessions.rootId}`): SQL {
  return sql`${column} IN (SELECT sp.root_id FROM sessions sp WHERE sp.device_id = ${grant.deviceId} AND sp.user_id = ${grant.userId})`
}

/** SQL: the space's own trees, and the chats its Assistant started since it
 *  runs here (an agent key reads and steers what it started, nothing else). */
export function spaceReachCondition(grant: SpaceGrant, column: SQL = sql`${sessions.rootId}`): SQL {
  const own = spaceTreeCondition(grant, column)
  if (!grant.assistantId) return own
  return or(
    own,
    sql`${column} IN (SELECT st.id FROM sessions st WHERE st.parent_id IS NULL AND st.user_id = ${grant.userId} AND st.started_by_agent_id = ${grant.assistantId} AND st.created_at >= ${grant.createdAt.toISOString()}::timestamptz)`
  )!
}

/** Whether `session` runs ON the space (a device procedure's session). */
export const runsOnSpace = (grant: SpaceGrant, session: Pick<Session, `deviceId` | `userId`>): boolean =>
  session.deviceId === grant.deviceId && session.userId === grant.userId

/** The space's own root ids (for a shape's where clause), sorted. */
export async function spaceRootIds(exec: SpaceReader, grant: SpaceGrant): Promise<string[]> {
  const rows = await exec
    .select({ rootId: sessions.rootId })
    .from(sessions)
    .where(and(eq(sessions.deviceId, grant.deviceId), eq(sessions.userId, grant.userId)))
  return [...new Set(rows.flatMap((row) => (row.rootId ? [row.rootId] : [])))].sort()
}

/** The sessions whose issue links the space syncs: its roots and the live
 *  sessions of their trees (bounded by the tree caps), sorted. */
export async function spaceSessionIds(exec: SpaceReader, grant: SpaceGrant): Promise<string[]> {
  const roots = await spaceRootIds(exec, grant)
  if (roots.length === 0) return []
  const live = await exec
    .select({ id: sessions.id })
    .from(sessions)
    .where(and(inArray(sessions.rootId, roots), isNull(sessions.subagentKey), sql`${sessions.status} <> 'ended'`))
  return [...new Set([...roots, ...live.map((row) => row.id)])].sort()
}

/** Whether `rootId` is one of the space's trees (`reach` = or a chat its
 *  Assistant started). */
export async function rootInSpace(exec: SpaceReader, grant: SpaceGrant, rootId: string, how: `own` | `reach`): Promise<boolean> {
  const condition = how === `own` ? spaceTreeCondition(grant) : spaceReachCondition(grant)
  const [row] = await exec
    .select({ id: sessions.id })
    .from(sessions)
    .where(and(eq(sessions.id, rootId), condition))
    .limit(1)
  return !!row
}

// ── installs ──────────────────────────────────────────────────────────────

/** Of `installIds` (space installs), those their member let in: an approval
 *  of kind `space` names them (`approveInstall` checked its signature). */
export async function approvedSpaceInstallIds(exec: SpaceReader, installIds: readonly string[]): Promise<Set<string>> {
  if (installIds.length === 0) return new Set()
  const rows = await exec
    .select({ subjectInstallId: installApprovals.subjectInstallId })
    .from(installApprovals)
    .where(and(inArray(installApprovals.subjectInstallId, [...installIds]), eq(installApprovals.kind, SPACE_APPROVAL_KIND)))
  return new Set(rows.map((row) => row.subjectInstallId).filter((id) => installIds.includes(id)))
}
