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
import type { TeamRole } from "@/lib/domain"

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
  if (base.startsWith(`https://`))
    return `wss://${base.slice(`https://`.length)}`
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
      teamId: string
      sessionId: string
    }
  | {
      kind: `viewer`
      userId: string
      teamId: string
      sessionId: string
      role: TeamRole
      /** Display name (or email), shown in viewer presence. */
      name: string
      /** The caller owns the coding_sessions row — grants steer regardless
       *  of team role (you may always steer your own session). */
      isSessionOwner?: boolean
    }

// Team owners may steer, and so may the coding session's own starter
// (isSessionOwner); plain members watch. (The role enum is owner|member only
// — there is no admin role.)
export function viewerPermFor(
  role: TeamRole,
  isSessionOwner = false
): SteerPerm {
  return role === `owner` || isSessionOwner ? `steer` : `view`
}

export function buildSteerTicketClaims(
  seed: SteerTicketSeed,
  nowSeconds = Math.floor(Date.now() / 1000)
): SteerTicketClaims {
  const base = {
    sub: seed.userId,
    iat: nowSeconds,
    exp: nowSeconds + STEER_TICKET_TTL_SECONDS,
  }
  switch (seed.kind) {
    case `control`:
      // Control tickets are account-scoped, not team-scoped — team is the
      // empty string by convention (see SteerTicketClaims docs).
      return {
        ...base,
        team: ``,
        role: `control`,
        perm: `steer`,
        ...(seed.deviceLabel ? { deviceLabel: seed.deviceLabel } : {}),
      }
    case `publisher`:
      return {
        ...base,
        team: seed.teamId,
        sessionId: seed.sessionId,
        role: `publisher`,
        perm: `steer`,
      }
    case `viewer`:
      return {
        ...base,
        team: seed.teamId,
        sessionId: seed.sessionId,
        name: seed.name,
        role: `viewer`,
        perm: viewerPermFor(seed.role, seed.isSessionOwner),
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
  /** EXP-201: agent CLIs the device advertised (`claude`/`codex`/`pi`).
   * Absent from an old relay ⇒ treat as claude-only. */
  agents?: string[]
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

/**
 * Launch options a remote start may carry (EXP-149) — the client's
 * Start-coding dialog choices. All optional; an absent field means "desktop
 * settings default" (and plan mode OFF). `effort: ""` is an explicit
 * "CLI default" (omit --effort), distinct from absent.
 */
export interface SteerStartOptions {
  /** EXP-201: the agent CLI to launch (`claude`/`codex`/`pi`); absent =
   * claude (the pre-EXP-201 behavior on every desktop). */
  agent?: string
  model?: string
  effort?: string
  ultracode?: boolean
  planMode?: boolean
  /** EXP-201: full permission bypass instead of the agent's guarded auto
   * mode. Absent = desktop settings default. */
  skipPermissions?: boolean
}

/**
 * The repo group a BATCH remote start carries. Resolved server-side (from the
 * batch's shared board repository) because the desktop syncs no repositories
 * collection — the relay frame must be "fat" enough for the launcher to clone
 * without a lookup. NEVER includes installationId: that is a server-only
 * secret and must never ride the relay.
 */
export interface SteerStartRepo {
  repositoryId: string
  fullName: string
  defaultBranch: string
}

/**
 * The subject of a remote start: either a single issue (wire-unchanged) or a
 * batch of issues sharing one team + repo group. Exactly one form.
 */
export type SteerStartSubject =
  | { issueId: string }
  | { issueIds: string[]; teamId: string; repo: SteerStartRepo }

/** POST /start — route a remote start to the device's control socket.
 * Undefined option fields are dropped by JSON.stringify — never sent. */
export async function relayPostStart(
  config: SteerRelayConfig,
  body: { userId: string; deviceId: string } & SteerStartSubject &
    SteerStartOptions,
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
