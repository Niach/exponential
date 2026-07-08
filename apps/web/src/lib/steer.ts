// Steer relay helpers — the pure core of the `steer` tRPC router (masterplan
// §3.5). Ticket-claim composition, permission mapping, relay URL derivation,
// and the secret-authed server-to-server relay HTTP calls live here so they
// are unit-testable without a DB or a live relay. The wire truth for
// everything steer is apps/steer-relay/src/protocol.ts + the ticket format in
// packages/steer-ticket.

import {
  signSteerTicket,
  type SteerPerm,
  type SteerTicketClaims,
} from "@exp/steer-ticket"
import type { WorkspaceRole } from "@/lib/domain"

// ── Config (env) ──────────────────────────────────────────────────────────────

export interface SteerRelayConfig {
  url: string
  secret: string
}

// Enabled iff BOTH STEER_RELAY_URL and STEER_RELAY_SECRET are set — mirrors
// how PUSH_RELAY_URL unset disables push without breaking anything.
export function getSteerRelayConfig(
  env: Record<string, string | undefined> = process.env
): SteerRelayConfig | null {
  const url = env.STEER_RELAY_URL?.trim()
  const secret = env.STEER_RELAY_SECRET?.trim()
  if (!url || !secret) return null
  return { url, secret }
}

// ── Relay URL derivation ──────────────────────────────────────────────────────

// STEER_RELAY_URL may be given with an http(s) or ws(s) scheme; sockets need
// ws(s) and the admin HTTP endpoints need http(s), so translate both ways.

function stripTrailingSlashes(url: string): string {
  return url.trim().replace(/\/+$/, ``)
}

export function steerWsBase(relayUrl: string): string {
  const base = stripTrailingSlashes(relayUrl)
  if (base.startsWith(`https://`)) return `wss://${base.slice(`https://`.length)}`
  if (base.startsWith(`http://`)) return `ws://${base.slice(`http://`.length)}`
  return base
}

export function steerHttpBase(relayUrl: string): string {
  const base = stripTrailingSlashes(relayUrl)
  if (base.startsWith(`wss://`)) return `https://${base.slice(`wss://`.length)}`
  if (base.startsWith(`ws://`)) return `http://${base.slice(`ws://`.length)}`
  return base
}

// The full dial URL. The relay reads the ticket from the query string
// (browsers can't set WebSocket headers): GET {relay}/ws?ticket=<ticket>.
export function steerTicketUrl(relayUrl: string, ticket: string): string {
  return `${steerWsBase(relayUrl)}/ws?ticket=${encodeURIComponent(ticket)}`
}

// ── Ticket claims + minting ───────────────────────────────────────────────────

/** Connect window in seconds; the socket outlives it once established. */
export const STEER_TICKET_TTL_SECONDS = 60

export type SteerTicketSeed =
  | { kind: `control`; userId: string; deviceLabel?: string }
  | {
      kind: `publisher`
      userId: string
      workspaceId: string
      sessionId: string
    }
  | {
      kind: `viewer`
      userId: string
      workspaceId: string
      sessionId: string
      role: WorkspaceRole
      /** Display name (or email), shown in viewer presence. */
      name: string
    }
  // Anonymous public-activity audience (feedback boards with
  // publicShowCoding='live'). Read-only, activity-channel-only; the tRPC mint
  // verifies the project's toggle — no user identity involved.
  | { kind: `public_viewer`; sessionId: string }

// Workspace owners may steer; plain members watch. (The role enum is
// owner|member only — there is no admin role.)
export function viewerPermFor(role: WorkspaceRole): SteerPerm {
  return role === `owner` ? `steer` : `view`
}

export function buildSteerTicketClaims(
  seed: SteerTicketSeed,
  nowSeconds = Math.floor(Date.now() / 1000)
): SteerTicketClaims {
  const base = {
    sub: seed.kind === `public_viewer` ? `anon` : seed.userId,
    iat: nowSeconds,
    exp: nowSeconds + STEER_TICKET_TTL_SECONDS,
  }
  switch (seed.kind) {
    case `public_viewer`:
      return {
        ...base,
        ws: ``,
        sessionId: seed.sessionId,
        role: `public_viewer`,
        perm: `view`,
      }
    case `control`:
      // Control tickets are account-scoped, not workspace-scoped — ws is the
      // empty string by convention (see SteerTicketClaims docs).
      return {
        ...base,
        ws: ``,
        role: `control`,
        perm: `steer`,
        ...(seed.deviceLabel ? { deviceLabel: seed.deviceLabel } : {}),
      }
    case `publisher`:
      return {
        ...base,
        ws: seed.workspaceId,
        sessionId: seed.sessionId,
        role: `publisher`,
        perm: `steer`,
      }
    case `viewer`:
      return {
        ...base,
        ws: seed.workspaceId,
        sessionId: seed.sessionId,
        name: seed.name,
        role: `viewer`,
        perm: viewerPermFor(seed.role),
      }
  }
}

export type MintSteerTicketResult =
  | { disabled: true }
  | { ticket: string; url: string }

export function mintSteerTicket(
  config: SteerRelayConfig | null,
  seed: SteerTicketSeed,
  nowSeconds?: number
): MintSteerTicketResult {
  if (!config) return { disabled: true }
  const ticket = signSteerTicket(
    buildSteerTicketClaims(seed, nowSeconds),
    config.secret
  )
  return { ticket, url: steerTicketUrl(config.url, ticket) }
}

// ── Relay admin HTTP (x-relay-secret, server-to-server) ───────────────────────

// Minimal structural fetch so tests can mock without constructing Responses.
interface RelayResponse {
  ok: boolean
  status: number
  json(): Promise<unknown>
}

export type RelayFetch = (
  url: string,
  init?: {
    method?: string
    headers?: Record<string, string>
    body?: string
  }
) => Promise<RelayResponse>

export interface SteerDevice {
  deviceId: string
  deviceLabel: string
  connectedAt: number
}

/** GET /devices/:userId — online desktops for the "Start on my desktop" picker. */
export async function relayGetDevices(
  config: SteerRelayConfig,
  userId: string,
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<{ devices: SteerDevice[] }> {
  const res = await fetchImpl(
    `${steerHttpBase(config.url)}/devices/${encodeURIComponent(userId)}`,
    { headers: { "x-relay-secret": config.secret } }
  )
  if (!res.ok) {
    throw new Error(`Steer relay /devices failed (${res.status})`)
  }
  const json = (await res.json()) as { devices?: SteerDevice[] }
  return { devices: json.devices ?? [] }
}

export type RelayStartResult =
  | { ok: true }
  | { ok: false; status: number; reason: string }

/** POST /start — route a remote start to the device's control socket. */
export async function relayPostStart(
  config: SteerRelayConfig,
  body: { userId: string; deviceId: string; issueId: string },
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<RelayStartResult> {
  const res = await fetchImpl(`${steerHttpBase(config.url)}/start`, {
    method: `POST`,
    headers: {
      "content-type": `application/json`,
      "x-relay-secret": config.secret,
    },
    body: JSON.stringify(body),
  })
  if (res.ok) return { ok: true }
  const json = (await res.json().catch(() => null)) as {
    error?: string
  } | null
  return { ok: false, status: res.status, reason: json?.error ?? `relay_error` }
}

/**
 * POST /sessions/:id/kill — best-effort kill-switch fan-out. Never throws: the
 * relay is additive, never load-bearing; the DB row flip (which the desktop
 * watches over Electric) is the durable abort path.
 */
export async function relayPostKill(
  config: SteerRelayConfig,
  sessionId: string,
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<{ delivered: boolean }> {
  try {
    const res = await fetchImpl(
      `${steerHttpBase(config.url)}/sessions/${encodeURIComponent(sessionId)}/kill`,
      { method: `POST`, headers: { "x-relay-secret": config.secret } }
    )
    if (!res.ok) return { delivered: false }
    const json = (await res.json().catch(() => null)) as {
      delivered?: boolean
    } | null
    return { delivered: json?.delivered === true }
  } catch {
    return { delivered: false }
  }
}
