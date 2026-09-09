// EXP-792: the server side of a web-initiated, device-executed MCP OAuth
// sign-in. The server never holds a credential: it mints the `state`, queues
// the `mcp_oauth_start` command, relays the authorization code back to the
// device as `mcp_oauth_code` (the PKCE verifier lives on the device, so the
// code alone is useless here) and records the outcome on the flow row plus
// the per-device readiness matrix. Shared by the `mcpServers` router, the
// `devices.completeCommand` hook and the anonymous callback route — so this
// module imports NO router (devices.ts imports it).
import { randomBytes } from "node:crypto"
import { and, eq, inArray } from "drizzle-orm"
import type { Context } from "@/lib/trpc"
import { mcpOauthFlows, mcpServerReadiness } from "@/db/schema"
import { appBaseUrl } from "@/lib/notification-email-policy"

export type Db = Context[`db`]

/** A flow that has not finished within this window reads as failed
 * (`expired`); the callback refuses its state and beginOAuth mints a new one. */
export const MCP_OAUTH_FLOW_TTL_MS = 10 * 60_000

export const MCP_OAUTH_FLOW_STATUSES = [
  `pending`,
  `authorize_url`,
  `code_relayed`,
  `done`,
  `failed`,
] as const
export type McpOauthFlowStatus = (typeof MCP_OAUTH_FLOW_STATUSES)[number]

/** The statuses a flow may still move out of. */
export const MCP_OAUTH_FLOW_LIVE_STATUSES = [
  `pending`,
  `authorize_url`,
  `code_relayed`,
] as const

export type McpOauthRedirect = `hosted` | `loopback`

/** 32 random bytes, base64url — the one value both the callback and the
 * device key the flow on. */
export function newFlowState(): string {
  return randomBytes(32).toString(`base64url`)
}

export function flowIsTerminal(status: string): boolean {
  return status === `done` || status === `failed`
}

export function flowIsExpired(createdAt: Date, now = new Date()): boolean {
  return now.getTime() - createdAt.getTime() >= MCP_OAUTH_FLOW_TTL_MS
}

/** The callback accepts a state only while the device may still be waiting
 * for a code: not yet relayed, not finished, inside the TTL. */
export function flowAcceptsCode(
  flow: { status: string; createdAt: Date },
  now = new Date()
): boolean {
  return (
    (flow.status === `pending` || flow.status === `authorize_url`) &&
    !flowIsExpired(flow.createdAt, now)
  )
}

/** beginOAuth reuses an in-flight flow instead of queueing a second
 * `mcp_oauth_start` at the device. */
export function flowIsPending(
  flow: { status: string; createdAt: Date },
  now = new Date()
): boolean {
  return !flowIsTerminal(flow.status) && !flowIsExpired(flow.createdAt, now)
}

const PRIVATE_HOST =
  /^(localhost|.*\.localhost|127\.\d+\.\d+\.\d+|10\.\d+\.\d+\.\d+|192\.168\.\d+\.\d+|172\.(1[6-9]|2\d|3[01])\.\d+\.\d+|169\.254\.\d+\.\d+|0\.0\.0\.0|\[::1\]|\[fc[0-9a-f]{2}:.*\]|\[fd[0-9a-f]{2}:.*\])$/i

/** Whether an OAuth provider on the public internet can redirect back to
 * this instance: https and a routable host. A LAN self-host or a dev box
 * fails this, and the device's own loopback listener takes the code instead. */
export function isPublicHttpsBase(base: string): boolean {
  let url: URL
  try {
    url = new URL(base)
  } catch {
    return false
  }
  if (url.protocol !== `https:`) return false
  return !PRIVATE_HOST.test(url.hostname) && !PRIVATE_HOST.test(url.host)
}

export function defaultRedirect(base = appBaseUrl()): McpOauthRedirect {
  return isPublicHttpsBase(base) ? `hosted` : `loopback`
}

export function hostedCallbackUrl(base = appBaseUrl()): string {
  return `${base.replace(/\/$/, ``)}/api/mcp-oauth/callback`
}

/** The `redirectUri` an `mcp_oauth_start` payload carries: the hosted
 * callback, or the literal `loopback` (the device binds 127.0.0.1 itself). */
export function redirectUriFor(redirect: McpOauthRedirect): string {
  return redirect === `hosted` ? hostedCallbackUrl() : `loopback`
}

/** ISO-normalize a device-reported expiry; junk degrades to null (the UI
 * treats an unknown expiry as "no warning", never as expired). */
export function expiresAtOrNull(value: unknown): Date | null {
  if (typeof value !== `string` || value.length === 0) return null
  const at = new Date(value)
  return Number.isNaN(at.getTime()) ? null : at
}

export interface ReadinessUpsert {
  serverId: string
  deviceRowId: string
  userId: string
  ready: boolean
  expiresAt?: Date | null
  error?: string | null
}

/** One (server, device) readiness row, replaced wholesale — `error` and
 * `expiresAt` are the report's, not merged with the previous row's. */
export async function upsertReadiness(
  db: Db,
  entry: ReadinessUpsert,
  now = new Date()
): Promise<void> {
  const values = {
    serverId: entry.serverId,
    deviceRowId: entry.deviceRowId,
    userId: entry.userId,
    ready: entry.ready,
    expiresAt: entry.expiresAt ?? null,
    error: entry.error ?? null,
    checkedAt: now,
  }
  await db
    .insert(mcpServerReadiness)
    .values(values)
    .onConflictDoUpdate({
      target: [mcpServerReadiness.serverId, mcpServerReadiness.deviceRowId],
      set: {
        userId: values.userId,
        ready: values.ready,
        expiresAt: values.expiresAt,
        error: values.error,
        checkedAt: now,
        updatedAt: now,
      },
    })
}

/** Terminal transition for a flow row. Only live rows move, so a duplicate
 * completion (heartbeat redelivery racing the first, or finishOAuth after
 * the code command already closed it) is a no-op. */
export async function finishFlow(
  db: Db,
  flowId: string,
  outcome: { ok: true } | { ok: false; error: string },
  now = new Date()
): Promise<void> {
  await db
    .update(mcpOauthFlows)
    .set({
      status: outcome.ok ? `done` : `failed`,
      error: outcome.ok ? null : outcome.error.slice(0, 500),
      completedAt: now,
    })
    .where(
      and(
        eq(mcpOauthFlows.id, flowId),
        inArray(mcpOauthFlows.status, [...MCP_OAUTH_FLOW_LIVE_STATUSES])
      )
    )
}

/** Parse the device's JSON `message` on a command completion defensively:
 * an older build, or a bare error string, must never throw here. */
export function parseCompletionMessage(
  message: string | undefined
): Record<string, unknown> | null {
  if (!message) return null
  try {
    const parsed: unknown = JSON.parse(message)
    return parsed && typeof parsed === `object` && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : null
  } catch {
    return null
  }
}

/** The `devices.completeCommand` hook for the two OAuth command kinds.
 * `mcp_oauth_start`: ok + `{phase:"authorize", url}` → the flow shows the
 * authorize URL (the web opens it); ok without a usable URL or ok=false →
 * failed. `mcp_oauth_code`: ok → done + readiness ready (expiresAt from
 * `{phase:"done", expiresAt}`); ok=false → failed + readiness not ready with
 * the device's reason. The flow is located by `payload.state`; an unknown
 * state (flow purged, foreign payload) is a no-op. */
export async function applyMcpOauthCommandCompletion(
  db: Db,
  command: {
    kind: string
    payload: Record<string, string>
    ok: boolean
    message?: string
  },
  now = new Date()
): Promise<void> {
  if (command.kind !== `mcp_oauth_start` && command.kind !== `mcp_oauth_code`) {
    return
  }
  const state = command.payload?.state
  if (!state) return
  const [flow] = await db
    .select({
      id: mcpOauthFlows.id,
      status: mcpOauthFlows.status,
      serverId: mcpOauthFlows.serverId,
      deviceRowId: mcpOauthFlows.deviceRowId,
      userId: mcpOauthFlows.userId,
    })
    .from(mcpOauthFlows)
    .where(eq(mcpOauthFlows.state, state))
    .limit(1)
  if (!flow || flowIsTerminal(flow.status)) return

  const body = parseCompletionMessage(command.message)
  if (command.kind === `mcp_oauth_start`) {
    const url = typeof body?.url === `string` ? body.url : ``
    if (command.ok && body?.phase === `authorize` && /^https?:\/\//.test(url)) {
      await db
        .update(mcpOauthFlows)
        .set({ status: `authorize_url`, authorizeUrl: url.slice(0, 8192) })
        .where(and(eq(mcpOauthFlows.id, flow.id), eq(mcpOauthFlows.status, `pending`)))
      return
    }
    await finishFlow(
      db,
      flow.id,
      {
        ok: false,
        error: command.ok
          ? `device returned no authorize url`
          : command.message || `device refused the sign-in`,
      },
      now
    )
    return
  }

  // mcp_oauth_code
  const readiness = {
    serverId: flow.serverId,
    deviceRowId: flow.deviceRowId,
    userId: flow.userId,
  }
  if (command.ok) {
    await finishFlow(db, flow.id, { ok: true }, now)
    await upsertReadiness(
      db,
      { ...readiness, ready: true, expiresAt: expiresAtOrNull(body?.expiresAt) },
      now
    )
    return
  }
  const error = (command.message || `token exchange failed`).slice(0, 500)
  await finishFlow(db, flow.id, { ok: false, error }, now)
  await upsertReadiness(db, { ...readiness, ready: false, error }, now)
}
