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
 *  row naming the machine", which anyone can register). `revoking` = that
 *  row shared the session's team until the caller's write just now
 *  (unshare, leave): the rule as of before the write. */
export function hostRowCondition(
  alias: string,
  s: typeof sessions = sessions,
  opts: { revoking?: boolean } = {}
): SQL {
  const h = sql.raw(alias)
  const shares = opts.revoking ? sql`TRUE` : sql`${s.teamId} = ANY(${h}.shared_team_ids)`
  return sql`(${h}.device_id = ${s.deviceId} AND (
    ${h}.user_id = ${s.userId}
    OR (${shares}
      AND NOT EXISTS (SELECT 1 FROM ${devices} o WHERE o.device_id = ${s.deviceId} AND o.user_id = ${s.userId})
      AND NOT EXISTS (SELECT 1 FROM ${devices} o WHERE o.device_id = ${s.deviceId} AND o.id <> ${h}.id AND ${s.teamId} = ANY(o.shared_team_ids) AND (o.created_at, o.id) < (${h}.created_at, ${h}.id)))))`
}

/** SQL: `userId` is the host `resolveSessionHost` picks for the `sessions`
 *  row it is joined to, through its OWN devices row for that machine
 *  (FEED-147). `devices.device_id` is unique per user only: anyone can
 *  register a box's id, so a heartbeat, an unshare, a leave or an account
 *  deletion speaks for a session only when the same rule the device
 *  procedures use (`assertDeviceHost`) names its sender. */
export function hostedByCondition(
  userId: string,
  s: typeof sessions = sessions,
  opts: { revoking?: boolean } = {}
): SQL {
  return sql`EXISTS (SELECT 1 FROM ${devices} h WHERE h.user_id = ${userId} AND ${hostRowCondition(`h`, s, opts)})`
}

/** FEED-148: another account already shares `deviceId` into one of
 *  `teamIds`. Call it after `lockDeviceShares`, in the transaction that
 *  writes the caller's share. */
export async function shareContested(
  tx: Reader,
  userId: string,
  deviceId: string,
  teamIds: readonly string[]
): Promise<boolean> {
  if (teamIds.length === 0) return false
  const [row] = await tx
    .select({ id: devices.id })
    .from(devices)
    .where(and(eq(devices.deviceId, deviceId), ne(devices.userId, userId), arrayOverlaps(devices.sharedTeamIds, [...teamIds])))
    .limit(1)
  return !!row
}

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
