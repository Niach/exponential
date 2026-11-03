// VAPP-239 (eApps M2, apps.md §8/§13): an app call to a DEVICE-ROUTED
// upstream (@exp/emcp upstream.json `device`: its `$comment` is the spec).
// The server row names a member's device; only that member's own calls (an
// automation's sponsor included) route there. One call = one
// `mcp_device_calls` row; the relay carries its id down the device's control
// socket, the device claims it over its own authenticated API (credential
// resolved at claim time, never stored), runs it through the `emcp` client
// and seals its answer to the reply key whose secret lives only here, in the
// awaiting request.
import { randomBytes, randomUUID } from "node:crypto"
import { and, eq, gt, isNull, lt, sql } from "drizzle-orm"
import {
  UPSTREAM_DEVICE,
  UPSTREAM_ERRORS,
  deviceCallOutcome,
  upstreamRoute,
  type DeviceAnswer,
  type DeviceCallOutcome,
  type DeviceRelayOutcome,
  type UpstreamRoute,
} from "@exp/emcp/upstream"
import { db as defaultDb } from "@/db/connection"
import { devices, mcpDeviceCalls, mcpServers, type McpDeviceCall, type McpServer } from "@/db/schema"
import { resolveForLaunch, type ResolvedMcpServer } from "@/lib/emcp/upstream/credentials"
import { pushOpen, pushPublicKey } from "@/lib/push-seal"
import { getSteerRelayConfig, relayPostUpstreamCall, type RelayUpstreamOutcome } from "@/lib/steer"

type Db = typeof defaultDb

/** The device a server row names, as the router and the relay need it. */
export interface ServerDevice {
  id: string
  ownerId: string
  label: string
  /** The steer deviceId the relay knows the device by. */
  steerDeviceId: string
}

export type RoutedServerRow = Pick<McpServer, `id` | `name` | `transport` | `teamId`> & { device: ServerDevice | null }

/** The team's server `name` with the device it names, or null. */
export async function loadRoutedServer(teamId: string, name: string, db: Db = defaultDb): Promise<RoutedServerRow | null> {
  const [row] = await db
    .select({
      id: mcpServers.id,
      name: mcpServers.name,
      transport: mcpServers.transport,
      teamId: mcpServers.teamId,
      deviceId: devices.id,
      ownerId: devices.userId,
      label: devices.label,
      steerDeviceId: devices.deviceId,
    })
    .from(mcpServers)
    .leftJoin(devices, eq(devices.id, mcpServers.deviceId))
    .where(and(eq(mcpServers.teamId, teamId), eq(mcpServers.name, name)))
    .limit(1)
  if (!row) return null
  const device =
    row.deviceId && row.ownerId && row.steerDeviceId
      ? { id: row.deviceId, ownerId: row.ownerId, label: row.label ?? `a device`, steerDeviceId: row.steerDeviceId }
      : null
  return { id: row.id, name: row.name, transport: row.transport, teamId: row.teamId, device }
}

/** Where a call by `callerId` runs (the fixture's `upstreamRoute`). */
export function routeServer(server: RoutedServerRow, callerId: string): UpstreamRoute {
  return upstreamRoute({ name: server.name, transport: server.transport, device: server.device }, callerId)
}

/** The read/call check the device runs on the tool's live annotations. */
export type DeviceVeto = { kind: `read`; sources: string[] } | { kind: `call`; access: string }

export interface DeviceCallDeps {
  relay?: (callId: string, device: ServerDevice) => Promise<RelayUpstreamOutcome>
  now?: () => Date
}

const defaultRelay = (callId: string, device: ServerDevice): Promise<RelayUpstreamOutcome> => {
  const config = getSteerRelayConfig()
  if (!config) return Promise.resolve({ delivered: false, reason: `device_offline` })
  return relayPostUpstreamCall(config, { userId: device.ownerId, deviceId: device.steerDeviceId }, callId)
}

/** The opened answer, or null when it does not open or parse. */
export function openDeviceAnswer(secret: Uint8Array, sealed: string): DeviceAnswer | null {
  try {
    const bytes = pushOpen(secret, new Uint8Array(Buffer.from(sealed, `base64url`)))
    if (bytes.length > UPSTREAM_DEVICE.maxAnswerBytes) return null
    const answer = JSON.parse(Buffer.from(bytes).toString(`utf8`)) as Record<string, unknown>
    if (answer?.ok === true && answer.result && typeof answer.result === `object` && !Array.isArray(answer.result)) {
      return { ok: true, result: answer.result as Record<string, unknown> }
    }
    if (answer?.ok === false && (answer.reason === `vetoed` || answer.reason === `refused` || answer.reason === `failed`)) {
      return { ok: false, reason: answer.reason, error: typeof answer.error === `string` ? answer.error.slice(0, 500) : `the call failed` }
    }
    return null
  } catch {
    return null
  }
}

/** Run one tool of a device-routed server as `userId` (its device's owner:
 *  the caller routed it). Never throws. */
export async function callOnDevice(
  input: { userId: string; server: RoutedServerRow & { device: ServerDevice }; tool: string; args: Record<string, unknown>; veto: DeviceVeto },
  deps: DeviceCallDeps = {},
  db: Db = defaultDb
): Promise<DeviceCallOutcome> {
  const { server, userId } = input
  const now = deps.now?.() ?? new Date()
  // Fail fast when the member is not connected: the device would only answer
  // the same refusal after a round trip.
  const launch = await resolveForLaunch(db, { userId, teamIds: [server.teamId], serverIds: [server.id] }, now)
  if (!launch.servers[0]) {
    return { error: UPSTREAM_ERRORS.disconnected, message: `connect your ${server.name} account first` }
  }
  const secret = new Uint8Array(randomBytes(32))
  const callId = randomUUID()
  await db.insert(mcpDeviceCalls).values({
    id: callId,
    teamId: server.teamId,
    userId,
    serverId: server.id,
    deviceId: server.device.id,
    tool: input.tool,
    args: input.args,
    veto: input.veto,
    replyPublic: Buffer.from(pushPublicKey(secret)).toString(`base64url`),
    expiresAt: new Date(now.getTime() + UPSTREAM_DEVICE.ttlSeconds * 1000),
  })
  try {
    const relayed = await (deps.relay ?? defaultRelay)(callId, server.device)
    const relay: DeviceRelayOutcome = relayed.delivered ? { delivered: true } : { delivered: false, reason: relayed.reason }
    const answer = relayed.delivered ? openDeviceAnswer(secret, relayed.sealed) : null
    return deviceCallOutcome(server.name, server.device.label, relay, answer)
  } finally {
    await db.delete(mcpDeviceCalls).where(eq(mcpDeviceCalls.id, callId)).catch(() => {})
  }
}

/** What a device gets for a claimed call: the server with the member's
 *  credential resolved now (null + `skipped` when not connected). */
export interface ClaimedDeviceCall {
  server: ResolvedMcpServer | null
  skipped: string | null
  tool: string
  args: Record<string, unknown>
  veto: DeviceVeto
  replyPublic: string
}

/** Claim `callId` for the member's device `steerDeviceId`: their own,
 *  unexpired, unclaimed call on that device, else null (one answer for
 *  every miss). */
export async function claimDeviceCall(
  input: { callId: string; userId: string; steerDeviceId: string },
  now: Date = new Date(),
  db: Db = defaultDb
): Promise<ClaimedDeviceCall | null> {
  const [call] = (await db
    .update(mcpDeviceCalls)
    .set({ claimedAt: now })
    .where(
      and(
        eq(mcpDeviceCalls.id, input.callId),
        eq(mcpDeviceCalls.userId, input.userId),
        isNull(mcpDeviceCalls.claimedAt),
        gt(mcpDeviceCalls.expiresAt, now),
        sql`${mcpDeviceCalls.deviceId} in (select ${devices.id} from ${devices} where ${devices.userId} = ${input.userId} and ${devices.deviceId} = ${input.steerDeviceId})`
      )
    )
    .returning()) as McpDeviceCall[]
  if (!call) return null
  // The row must still route to this device (an owner may have moved it).
  const [server] = await db
    .select({ id: mcpServers.id })
    .from(mcpServers)
    .where(and(eq(mcpServers.id, call.serverId), eq(mcpServers.deviceId, call.deviceId)))
    .limit(1)
  if (!server) return null
  const launch = await resolveForLaunch(db, { userId: call.userId, teamIds: [call.teamId], serverIds: [call.serverId] }, now)
  return {
    server: launch.servers[0] ?? null,
    skipped: launch.servers[0] ? null : (launch.skipped[0]?.reason ?? `not connected`),
    tool: call.tool,
    args: call.args,
    veto: call.veto,
    replyPublic: call.replyPublic,
  }
}

/** The watchdog: drop calls past their life (a crashed request's leftovers). */
export async function pruneDeviceCalls(now: Date = new Date(), db: Db = defaultDb): Promise<number> {
  const gone = await db.delete(mcpDeviceCalls).where(lt(mcpDeviceCalls.expiresAt, now)).returning({ id: mcpDeviceCalls.id })
  return gone.length
}
