import { TRPCError } from "@trpc/server"
import { and, eq } from "drizzle-orm"
import { isPersistent } from "@exp/domain-contract/session-kind"
import { db } from "@/db/connection"
import { devices, type Device, type Session } from "@/db/schema"
import { carriedAccountOption, flattenAccounts } from "@/lib/accounts/account-option"
import { sessionDeviceOwner } from "@/lib/sessions/access"
import { effectiveCli, resolveSessionDevice } from "@/lib/sessions/send"
import { deviceRowIsOnline } from "@/lib/steer-devices"

// EXP-1252: moving a live session to another device. The request is a
// `sessions.send({sessionId, deviceId})` (synced `move_to_device_id` + the
// server-only `handoff.request`); the SOURCE host waits for a true idle,
// uploads the handoff package and calls `sessions.handoff`, which re-stamps
// the row onto the target (status queued). The refusal copy is byte-
// identical with every client's (`session-device-move` ×4).

export const MOVE_COPY = {
  steer: `Only people who can steer this session can move it.`,
  ended: `This session has ended.`,
  agent: `Agents move from Settings → Agents.`,
  aside: `An aside stays on its parent's device.`,
  offline: `This device is offline.`,
  visibility: `This device is shared differently, so the session can't move here.`,
  noLogin: `This device has no signed-in login for this agent.`,
} as const

export const movingCaption = (device: string) => `Moving to ${device}…`
export const movedText = (from: string, to: string) => `Moved from ${from} to ${to}`

type DeviceRow = Pick<
  Device,
  `deviceId` | `label` | `userId` | `lastSeenAt` | `agentAccounts` | `agentUsage` | `launchDefaults`
>

/** ONE machine's devices row (devices are per (user, deviceId)). */
export async function loadDeviceRow(deviceId: string, ownerId: string): Promise<DeviceRow | null> {
  const [row] = await db
    .select({
      deviceId: devices.deviceId,
      label: devices.label,
      userId: devices.userId,
      lastSeenAt: devices.lastSeenAt,
      agentAccounts: devices.agentAccounts,
      agentUsage: devices.agentUsage,
      launchDefaults: devices.launchDefaults,
    })
    .from(devices)
    .where(and(eq(devices.deviceId, deviceId), eq(devices.userId, ownerId)))
    .limit(1)
  return row ?? null
}

export function deviceName(row: Pick<DeviceRow, `label`> | null, deviceId: string): string {
  return row?.label?.trim() || deviceId
}

/** The agent a session runs: its `cli`, else its host device's default, else
 *  claude (contract order). */
async function sessionAgent(session: Session): Promise<string> {
  return (await effectiveCli(session)) ?? `claude`
}

/** The refusals that hold whatever the target is (agent, aside, ended). */
export function assertMovable(session: Session): void {
  if (isPersistent(session)) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: MOVE_COPY.agent })
  }
  if (session.aside) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: MOVE_COPY.aside })
  }
  if (session.status === `ended`) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MOVE_COPY.ended })
  }
}

export interface MoveTarget {
  deviceId: string
  name: string
  hostUserId: string | null
  ownerId: string
  /** The login to run on there (a profile id of the target), else null. */
  account: string | null
}

/**
 * The target of a move: visible to the session's STARTER exactly like a new
 * session's device (own, or shared with the team; `hostUserId` relative to
 * the starter), the SAME visibility (`shared`) as the session, online (when
 * `requireOnline`), and signed in for the session's agent. `account` = an
 * explicit pick (must be a signed-in login of that agent there), else the
 * source's login carried by email (`carriedAccountOption`), else null.
 */
export async function resolveMoveTarget(
  session: Session,
  deviceId: string,
  opts: { account?: string | null; requireOnline: boolean; now?: Date }
): Promise<MoveTarget> {
  let device: Awaited<ReturnType<typeof resolveSessionDevice>>
  try {
    device = await resolveSessionDevice(session.userId, session.teamId, deviceId)
  } catch {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MOVE_COPY.visibility })
  }
  if (device.shared !== session.shared) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MOVE_COPY.visibility })
  }
  const row = await loadDeviceRow(deviceId, device.ownerId)
  if (!row) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MOVE_COPY.offline })
  }
  if (opts.requireOnline && !deviceRowIsOnline(row.lastSeenAt, opts.now ?? new Date())) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MOVE_COPY.offline })
  }
  const agent = await sessionAgent(session)
  const options = flattenAccounts(row).filter((option) => option.agent === agent)
  if (options.length === 0) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MOVE_COPY.noLogin })
  }
  let account: string | null = null
  if (opts.account) {
    if (!options.some((option) => option.id === opts.account)) {
      throw new TRPCError({
        code: `BAD_REQUEST`,
        message: `That login is not signed in on this device`,
      })
    }
    account = opts.account
  } else {
    account = (await carriedAccount(session, agent, options)) ?? null
  }
  return {
    deviceId,
    name: deviceName(row, deviceId),
    hostUserId: device.hostUserId,
    ownerId: device.ownerId,
    account,
  }
}

/** The source's login (by email, off its host device's report) on the
 *  target's options; undefined when it has no address or no match. */
async function carriedAccount(
  session: Session,
  agent: string,
  options: ReturnType<typeof flattenAccounts>
): Promise<string | undefined> {
  if (!session.account) return undefined
  const source = await loadDeviceRow(session.deviceId, sessionDeviceOwner(session))
  const profile = source?.agentAccounts?.[agent]?.profiles?.find(
    (entry) => entry?.id === session.account
  )
  if (!profile?.email) return undefined
  return carriedAccountOption(options, { agent: agent as `claude`, email: profile.email })?.id
}
