import { TRPCError } from "@trpc/server"
import { and, arrayOverlaps, asc, eq, ne, sql, type SQL } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, sessions, type Session } from "@/db/schema"

/** Anything that can `select` (the pool or a transaction). */
type Reader = Pick<typeof db, `select`>

// NEW-1: the account that RUNS a session's device, derived from the devices
// rows (no `host_user_id` column any more). Devices rows are per (user,
// deviceId): the starter's own row for the session's `device_id` makes the
// starter the host; else the row whose owner shares the team hosts it
// (EXP-432: a teammate's start on a shared box). FEED-148: anyone can
// register a box's id, so a second account never gets a sharing row for a
// machine another account already shares into that team
// (`assertShareFree`); rows that predate the rule resolve to the EARLIEST
// sharing row (`created_at`, then `id`), never the lowest user id (a new
// account cannot make itself older than the box's own row).

export type HostKey = Pick<Session, `deviceId` | `userId` | `teamId`>

/** The sharing tiebreak every read uses (FEED-148): the first sharing row. */
export const SHARING_ORDER = [asc(devices.createdAt), asc(devices.id)] as const

/** The host account, or null when no devices row names the machine. */
export async function resolveSessionHost(
  session: HostKey,
  database: Reader = db
): Promise<string | null> {
  const rows = await database
    .select({ userId: devices.userId, sharedTeamIds: devices.sharedTeamIds })
    .from(devices)
    .where(eq(devices.deviceId, session.deviceId))
    .orderBy(...SHARING_ORDER)
  if (rows.some((row) => row.userId === session.userId)) return session.userId
  return rows.find((row) => (row.sharedTeamIds ?? []).includes(session.teamId))?.userId ?? null
}

/** The account that runs the session's device (the starter when nobody
 *  else's row names it: the device is gone or was never registered). */
export async function sessionDeviceOwner(session: HostKey, database: Reader = db): Promise<string> {
  return (await resolveSessionHost(session, database)) ?? session.userId
}

/** The device procedures (brief/claim/report/reply/consume): only the
 *  account the session's device runs under. */
export async function assertDeviceHost(userId: string, session: HostKey): Promise<void> {
  if ((await sessionDeviceOwner(session)) !== userId) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Only the device running this session can report for it`,
    })
  }
}

/** The caller may act AS the session over its MCP key: its starter, or
 *  the host of its device. */
export async function isSessionPrincipal(userId: string, session: HostKey): Promise<boolean> {
  return session.userId === userId || (await sessionDeviceOwner(session)) === userId
}

/** SQL: the `devices` row named `alias` is THE host row
 *  `resolveSessionHost` picks for the `sessions` row it is joined to: the
 *  starter's own row, else the first sharing row (FEED-148: never just "a
 *  row naming the machine", which anyone can register). A share write reads
 *  it BEFORE it writes, under `lockDeviceShares` (FEED-149). */
export function hostRowCondition(alias: string, s: typeof sessions = sessions): SQL {
  const h = sql.raw(alias)
  return sql`(${h}.device_id = ${s.deviceId} AND (
    ${h}.user_id = ${s.userId}
    OR (${s.teamId} = ANY(${h}.shared_team_ids)
      AND NOT EXISTS (SELECT 1 FROM ${devices} o WHERE o.device_id = ${s.deviceId} AND o.user_id = ${s.userId})
      AND NOT EXISTS (SELECT 1 FROM ${devices} o WHERE o.device_id = ${s.deviceId} AND o.id <> ${h}.id AND ${s.teamId} = ANY(o.shared_team_ids) AND (o.created_at, o.id) < (${h}.created_at, ${h}.id)))))`
}

/** SQL: `userId` is the host `resolveSessionHost` picks for the `sessions`
 *  row it is joined to, through its OWN devices row for that machine
 *  (FEED-147). `devices.device_id` is unique per user only: anyone can
 *  register a box's id, so a heartbeat, an unshare, a leave or an account
 *  deletion speaks for a session only when the same rule the device
 *  procedures use (`assertDeviceHost`) names its sender. */
export function hostedByCondition(userId: string, s: typeof sessions = sessions): SQL {
  return sql`EXISTS (SELECT 1 FROM ${devices} h WHERE h.user_id = ${userId} AND ${hostRowCondition(`h`, s)})`
}

/** FEED-148: the other accounts' rows sharing the caller's machine into
 *  one of `teamIds`, first sharer first. Call it after `lockDeviceShares`,
 *  in the transaction that writes the caller's share.
 *
 *  FEED-149 (squatting): a machine's id shows in team-visible sessions, so a
 *  teammate can register it and share it first. The box's own row predates
 *  anyone who learned its id there, so `older` (that row sorts before the
 *  caller's in `SHARING_ORDER`) decides: an older share holds and the
 *  caller's is refused (CONFLICT); a younger one is displaced by it. */
export async function otherShareRows(
  tx: Reader,
  row: { id: string; userId: string; deviceId: string },
  teamIds: readonly string[]
): Promise<ShareRow[]> {
  if (teamIds.length === 0) return []
  return tx
    .select({
      id: devices.id,
      userId: devices.userId,
      sharedTeamIds: devices.sharedTeamIds,
      older: sql<boolean>`(${devices.createdAt}, ${devices.id}) < (SELECT o.created_at, o.id FROM ${devices} o WHERE o.id = ${row.id})`,
    })
    .from(devices)
    .where(
      and(
        eq(devices.deviceId, row.deviceId),
        ne(devices.userId, row.userId),
        arrayOverlaps(devices.sharedTeamIds, [...teamIds])
      )
    )
    .orderBy(...SHARING_ORDER)
}

export type ShareRow = { id: string; userId: string; sharedTeamIds: string[] | null; older: boolean }

/** FEED-148: `teamIds` where the caller's row `rowId` is the FIRST sharing
 *  row of its machine (the host of teammates' starts there). */
export async function firstSharerTeams(
  tx: Reader,
  row: { id: string; deviceId: string },
  teamIds: readonly string[]
): Promise<string[]> {
  if (teamIds.length === 0) return []
  const rows = await tx
    .select({ id: devices.id, sharedTeamIds: devices.sharedTeamIds })
    .from(devices)
    .where(eq(devices.deviceId, row.deviceId))
    .orderBy(...SHARING_ORDER)
  return teamIds.filter(
    (teamId) => (rows.find((entry) => (entry.sharedTeamIds ?? []).includes(teamId))?.id ?? row.id) === row.id
  )
}

/** Serializes every share write on one machine id (FEED-148: two accounts
 *  racing to share the same box into one team). */
export async function lockDeviceShares(tx: Pick<typeof db, `execute`>, deviceId: string): Promise<void> {
  await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`device-share:${deviceId}`}))`)
}
