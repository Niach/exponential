// Steer relay helpers — the pure core of the `steer` tRPC router (masterplan
// §3.5). Ticket-claim composition, relay URL derivation, and the
// secret-authed server-to-server relay HTTP calls live here so they are
// unit-testable without a DB or a live relay. VAPP-43: the relay is device
// presence, control (check_in, kill, interrupt, compact routed down a
// device's control socket) and eTalk calls; it carries no session content
// (viewers read sessions over eNet from the device). The wire truth is
// apps/steer-relay/src/protocol.ts + the ticket format in
// packages/steer-ticket.

import { signSteerTicket, type SteerTicketClaims } from "@exp/steer-ticket"

// ── Config (env) ──────────────────────────────────────────────────────────────

export interface SteerRelayConfig {
  url: string
  secret: string
  /** EXP-504: base for the web app's OWN server-to-server relay calls
   * (`STEER_RELAY_INTERNAL_URL`, e.g. `http://steer-relay:4002` inside the
   * selfhost compose network). Unset ⇒ derive from `url` as before. Clients
   * always dial the public `url`; only this process uses the internal one. */
  internalUrl?: string
}

// Enabled iff BOTH STEER_RELAY_URL and STEER_RELAY_SECRET are set — mirrors
// how PUSH_RELAY_URL unset disables push without breaking anything.
export function getSteerRelayConfig(
  env: Record<string, string | undefined> = process.env
): SteerRelayConfig | null {
  const url = env.STEER_RELAY_URL?.trim()
  const secret = env.STEER_RELAY_SECRET?.trim()
  if (!url || !secret) return null
  const internalUrl = env.STEER_RELAY_INTERNAL_URL?.trim()
  return { url, secret, ...(internalUrl ? { internalUrl } : {}) }
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

// EXP-504: the http(s) base for the relay admin calls below (/devices,
// /kill, /interrupt, /compact, nudge). These are SERVER-to-server: deriving them from the
// public STEER_RELAY_URL sends the web container out through public DNS and
// back in through the reverse proxy, which half-breaks remote start on
// consumer routers without hairpin NAT (phones and desktops connect fine,
// the container's own calls don't). STEER_RELAY_INTERNAL_URL short-circuits
// that to the compose-network address; ticket dial URLs keep the public url.
export function steerServerHttpBase(config: SteerRelayConfig): string {
  return steerHttpBase(config.internalUrl ?? config.url)
}

// The full dial URL. The relay reads the ticket from the query string
// (browsers can't set WebSocket headers): GET {relay}/ws?ticket=<ticket>.
export function steerTicketUrl(relayUrl: string, ticket: string): string {
  return `${steerWsBase(relayUrl)}/ws?ticket=${encodeURIComponent(ticket)}`
}

// ── Ticket claims + minting ───────────────────────────────────────────────────

/** Connect window in seconds; the socket outlives it once established. */
export const STEER_TICKET_TTL_SECONDS = 60

// EXP-710: a control seed is the account and nothing else. VAPP-43: the
// `publisher` and `viewer` seeds are gone with the relay's session rooms.
export type SteerTicketSeed =
  | { kind: `control`; userId: string }
  // NEW-55: a voice call. The CALLER (a phone or web client streaming its
  // audio) names the call, the agent and the member's voice device; the relay
  // rings that device under the member (`deviceOwnerId`). The HOST (the voice
  // device answering) names the call alone — the relay admits it only when
  // its account owns the device the caller rang.
  | {
      kind: `caller`
      userId: string
      callId: string
      agentId: string
      deviceId: string
    }
  | { kind: `call_host`; userId: string; callId: string; deviceId: string }

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
      }
    case `caller`:
      return {
        ...base,
        team: ``,
        role: `caller`,
        callId: seed.callId,
        agentId: seed.agentId,
        deviceId: seed.deviceId,
        // The voice device is always the member's own desktop.
        deviceOwnerId: seed.userId,
      }
    case `call_host`:
      return { ...base, team: ``, role: `call_host`, callId: seed.callId, deviceId: seed.deviceId }
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
    signal?: AbortSignal
  }
) => Promise<RelayResponse>

/** VAPP-43: the device a session control call is routed to — the account
 * the device is registered under (the session's host) and its steer
 * deviceId. The relay sends the frame down that device's control socket. */
export interface RelayDevice {
  userId: string
  deviceId: string
}

/**
 * How long a kill fan-out may hang before we give up on it. The PR-merge path
 * (`applyPrMergeState`) awaits this fan-out post-commit inside the GitHub
 * webhook handler, and GitHub drops a delivery that takes longer than ~10s —
 * so an accepting-but-wedged relay must not hold the response open. Giving up
 * costs nothing: the durable abort path is the already-committed `ended` row.
 */
const RELAY_KILL_TIMEOUT_MS = 3_000

/**
 * POST /sessions/:id/kill `{userId, deviceId}` — best-effort kill-switch
 * down the session device's control socket. Never throws: the relay is
 * additive, never load-bearing; the DB row flip (which the desktop watches
 * over Electric) is the durable abort path.
 */
export async function relayPostKill(
  config: SteerRelayConfig,
  sessionId: string,
  device: RelayDevice,
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<{ delivered: boolean }> {
  try {
    const res = await fetchImpl(
      `${steerServerHttpBase(config)}/sessions/${encodeURIComponent(sessionId)}/kill`,
      {
        method: `POST`,
        headers: {
          "content-type": `application/json`,
          "x-relay-secret": config.secret,
        },
        body: JSON.stringify({ userId: device.userId, deviceId: device.deviceId }),
        signal: AbortSignal.timeout(RELAY_KILL_TIMEOUT_MS),
      }
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

const RELAY_INTERRUPT_TIMEOUT_MS = 3_000

/**
 * POST /sessions/:id/interrupt `{userId, deviceId}` — EXP-1291 Stop:
 * interrupt the running TURN (the relay sends `interrupt {sessionId}` down
 * the device's control socket; the session stays live). Best-effort and
 * never throws; a relay without the route 404s and reads as not-delivered.
 */
export async function relayPostInterrupt(
  config: SteerRelayConfig,
  sessionId: string,
  device: RelayDevice,
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<{ delivered: boolean }> {
  try {
    const res = await fetchImpl(
      `${steerServerHttpBase(config)}/sessions/${encodeURIComponent(sessionId)}/interrupt`,
      {
        method: `POST`,
        headers: {
          "content-type": `application/json`,
          "x-relay-secret": config.secret,
        },
        body: JSON.stringify({ userId: device.userId, deviceId: device.deviceId }),
        signal: AbortSignal.timeout(RELAY_INTERRUPT_TIMEOUT_MS),
      }
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

/**
 * EXP-936: the relay awaits the HOST's verdict before answering (its own
 * `COMPACT_VERDICT_TIMEOUT_MS` is 4s), so this sits above it: a structured
 * `no_verdict` from the relay beats an aborted fetch that says nothing.
 */
const RELAY_COMPACT_TIMEOUT_MS = 6_000

/** EXP-936: what the relay answers `POST /sessions/:id/compact` with, as the
 * handler consumes it. `delivered: false` = the session's device is not
 * connected (`device_offline`), the host never answered (older than the
 * frame), or another ask is still waiting — a tool ERROR on the web side,
 * never a verdict; the four refusal codes are the handler's own
 * (`sessionsCompactRefusals`). */
export type RelayCompactOutcome =
  | { delivered: false; reason: `device_offline` | `no_verdict` | `busy` | `relay_error` }
  | {
      delivered: true
      accepted: boolean
      refusedBecause?: `too_early` | `cooldown` | `not_own_session` | `unsupported_agent`
    }

/**
 * POST /sessions/:id/compact `{userId, deviceId, keep?}` — relay the run's
 * own `exponential_sessions_compact` down its device's control socket and
 * wait for the host's verdict (EXP-936). Never throws; an old relay's 404
 * and a hung one both read as not-delivered with a reason the handler can
 * name.
 */
export async function relayPostCompact(
  config: SteerRelayConfig,
  sessionId: string,
  device: RelayDevice,
  keep: string | undefined,
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<RelayCompactOutcome> {
  try {
    const res = await fetchImpl(
      `${steerServerHttpBase(config)}/sessions/${encodeURIComponent(sessionId)}/compact`,
      {
        method: `POST`,
        headers: {
          "content-type": `application/json`,
          "x-relay-secret": config.secret,
        },
        body: JSON.stringify({
          userId: device.userId,
          deviceId: device.deviceId,
          ...(keep === undefined ? {} : { keep }),
        }),
        signal: AbortSignal.timeout(RELAY_COMPACT_TIMEOUT_MS),
      }
    )
    if (!res.ok) return { delivered: false, reason: `relay_error` }
    const json = (await res.json().catch(() => null)) as
      | (Partial<RelayCompactOutcome> & { ok?: boolean })
      | null
    if (json?.delivered !== true) {
      const reason = json?.delivered === false ? json.reason : undefined
      return {
        delivered: false,
        reason:
          reason === `device_offline` || reason === `no_verdict` || reason === `busy`
            ? reason
            : `relay_error`,
      }
    }
    if (json.accepted === true) return { delivered: true, accepted: true }
    const code = json.refusedBecause
    return {
      delivered: true,
      accepted: false,
      refusedBecause:
        code === `too_early` ||
        code === `cooldown` ||
        code === `not_own_session` ||
        code === `unsupported_agent`
          ? code
          : `cooldown`,
    }
  } catch {
    return { delivered: false, reason: `relay_error` }
  }
}

const RELAY_NUDGE_TIMEOUT_MS = 3_000

/**
 * POST /devices/:userId/:deviceId/nudge — fire-and-forget `check_in` frame
 * (EXP-481): the server persisted new work for the device (a queued command,
 * edited launch defaults) and an online device should heartbeat NOW instead
 * of on its next cadence. Never throws; never load-bearing — the heartbeat
 * pickup is the durable path, the nudge only kills its latency.
 */
export async function relayPostNudge(
  config: SteerRelayConfig,
  userId: string,
  deviceId: string,
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<{ delivered: boolean }> {
  try {
    const res = await fetchImpl(
      `${steerServerHttpBase(config)}/devices/${encodeURIComponent(userId)}/${encodeURIComponent(deviceId)}/nudge`,
      {
        method: `POST`,
        headers: { "x-relay-secret": config.secret },
        signal: AbortSignal.timeout(RELAY_NUDGE_TIMEOUT_MS),
      }
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
