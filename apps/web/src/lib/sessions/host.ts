import { TRPCError } from "@trpc/server"
import { and, eq, or, sql, type SQL } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, sessions, type Session } from "@/db/schema"

/** Anything that can `select` (the pool or a transaction). */
type Reader = Pick<typeof db, `select`>

// NEW-1: the account that RUNS a session's device, derived from the devices
// rows (no `host_user_id` column any more). Devices rows are per (user,
// deviceId): the starter's own row for the session's `device_id` makes the
// starter the host; else the row whose owner shares the team hosts it
// (EXP-432: a teammate's start on a shared box). A machine has one owner in
// practice; two sharing rows resolve by the lower user id so every read
// agrees.

export type HostKey = Pick<Session, `deviceId` | `userId` | `teamId`>

/** The host account, or null when no devices row names the machine. */
export async function resolveSessionHost(
  session: HostKey,
  database: Reader = db
): Promise<string | null> {
  const rows = await database
    .select({ userId: devices.userId, sharedTeamIds: devices.sharedTeamIds })
    .from(devices)
    .where(eq(devices.deviceId, session.deviceId))
  if (rows.some((row) => row.userId === session.userId)) return session.userId
  const sharing = rows
    .filter((row) => (row.sharedTeamIds ?? []).includes(session.teamId))
    .map((row) => row.userId)
    .sort()
  return sharing[0] ?? null
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

/** SQL: a `devices` row hosts the `sessions` row it is joined to (the
 *  starter's own row, or one sharing the session's team). Several rows may
 *  match; use it for EXISTS / liveness checks, never to pick one row. */
export function hostDeviceCondition(
  s: typeof sessions = sessions,
  d: typeof devices = devices
): SQL {
  return and(
    eq(d.deviceId, s.deviceId),
    or(eq(d.userId, s.userId), sql`${s.teamId} = ANY(${d.sharedTeamIds})`)
  )!
}

/** SQL: `userId` is the host `resolveSessionHost` picks for the `sessions`
 *  row it is joined to, through its OWN devices row for that machine
 *  (FEED-147). `devices.device_id` is unique per user only: anyone can
 *  register a box's id, so a heartbeat speaks for a session only when the
 *  same rule the device procedures use (`assertDeviceHost`) names its
 *  sender: the starter's own row, else the lowest sharing row. */
export function hostedByCondition(userId: string, s: typeof sessions = sessions): SQL {
  return sql`EXISTS (SELECT 1 FROM ${devices} h WHERE h.device_id = ${s.deviceId} AND h.user_id = ${userId} AND (
    h.user_id = ${s.userId}
    OR (${s.teamId} = ANY(h.shared_team_ids)
      AND NOT EXISTS (SELECT 1 FROM ${devices} o WHERE o.device_id = ${s.deviceId} AND o.user_id = ${s.userId})
      AND NOT EXISTS (SELECT 1 FROM ${devices} o WHERE o.device_id = ${s.deviceId} AND ${s.teamId} = ANY(o.shared_team_ids) AND o.user_id COLLATE "C" < h.user_id COLLATE "C"))))`
}
