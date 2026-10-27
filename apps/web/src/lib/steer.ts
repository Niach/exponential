// Steer relay helpers — the pure core of the `steer` tRPC router (masterplan
// §3.5). Ticket-claim composition, relay URL derivation, and the
// secret-authed server-to-server relay HTTP calls live here so they are
// unit-testable without a DB or a live relay. The wire truth for everything
// steer is apps/steer-relay/src/protocol.ts + the ticket format in
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
// /start, /kill, nudge). These are SERVER-to-server: deriving them from the
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

// EXP-312: there is no view/steer perm distinction anymore — a live session
// is visible and steerable ONLY by its owner (the account that started it),
// enforced at mint time in the tRPC router. A ticket in hand IS full access
// to its session; the old `perm` claim is gone from the wire (every client
// floor is past the pre-EXP-312 builds).
// EXP-710: a control seed is the account and nothing else — the `deviceLabel`
// claim went with the presence metadata the relay stopped reading (EXP-672).
export type SteerTicketSeed =
  | { kind: `control`; userId: string }
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
      /** EXP-773: the machine that ran the session
       * (`sessions.device_id`). Riding the ticket is what lets the
       * relay ask that device for the transcript when the room is not up —
       * the transcript never leaves the device. Absent on rows that named no
       * device. */
      deviceId?: string
      /** EXP-432: the account that device is registered under (the run's host
       * for a shared-device run). Omitted = the ticket's own user. */
      deviceOwnerId?: string
      /** #EXP-1405: the row is still alive (idle): its replay is no ending
       *  (`SteerTicketClaims.live`). Only with `deviceId`. */
      live?: boolean
    }
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

/** The device claims of a VIEWER ticket: which machine the relay may ask
 *  for the stored transcript when no room is up. EXP-773: an ENDED session
 *  names its device. #EXP-1405: so does an IDLE one (started, not busy, not
 *  ended: its run went away between turns), marked `live` so the relay never
 *  answers its viewers with an ending; that is how web, desktop, iOS and
 *  Android all show the transcript of an idle session they did not run. A
 *  queued row (no journal yet) and a busy one (its publisher is the room)
 *  keep the plain `no_such_session` answer. */
export function viewerHistoryClaims(
  session: { status: string; busy: boolean; startedAt: Date | string | null; deviceId: string },
  deviceOwnerId: string
): { deviceId?: string; deviceOwnerId?: string; live?: boolean } {
  if (session.status === `ended`) return { deviceId: session.deviceId, deviceOwnerId }
  if (session.status === `queued` || session.busy || !session.startedAt) return {}
  return { deviceId: session.deviceId, deviceOwnerId, live: true }
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
      }
    case `publisher`:
      return {
        ...base,
        team: seed.teamId,
        sessionId: seed.sessionId,
        role: `publisher`,
      }
    case `viewer`:
      return {
        ...base,
        team: seed.teamId,
        sessionId: seed.sessionId,
        role: `viewer`,
        // EXP-773: only when the row named a device — an undefined key would
        // widen the ticket's JSON for nothing. The owner claim rides along
        // with it (EXP-432: presence is indexed by device owner) and only
        // when it differs from the ticket's own user.
        ...(seed.deviceId ? { deviceId: seed.deviceId } : {}),
        ...(seed.deviceId &&
        seed.deviceOwnerId &&
        seed.deviceOwnerId !== seed.userId
          ? { deviceOwnerId: seed.deviceOwnerId }
          : {}),
        ...(seed.deviceId && seed.live ? { live: true } : {}),
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

/**
 * How long a kill fan-out may hang before we give up on it. The PR-merge path
 * (`applyPrMergeState`) awaits this fan-out post-commit inside the GitHub
 * webhook handler, and GitHub drops a delivery that takes longer than ~10s —
 * so an accepting-but-wedged relay must not hold the response open. Giving up
 * costs nothing: the durable abort path is the already-committed `ended` row.
 */
const RELAY_KILL_TIMEOUT_MS = 3_000

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
      `${steerServerHttpBase(config)}/sessions/${encodeURIComponent(sessionId)}/kill`,
      {
        method: `POST`,
        headers: { "x-relay-secret": config.secret },
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
 * POST /sessions/:id/interrupt — EXP-1291 Stop: interrupt the running TURN
 * (the relay forwards its `interrupt` frame to the publisher; the session
 * stays live). Best-effort and never throws; a relay without the route 404s
 * and reads as not-delivered.
 */
export async function relayPostInterrupt(
  config: SteerRelayConfig,
  sessionId: string,
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<{ delivered: boolean }> {
  try {
    const res = await fetchImpl(
      `${steerServerHttpBase(config)}/sessions/${encodeURIComponent(sessionId)}/interrupt`,
      {
        method: `POST`,
        headers: { "x-relay-secret": config.secret },
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

const RELAY_INPUT_TIMEOUT_MS = 3_000

/**
 * POST /sessions/:id/input — inject text as user input into a live session's
 * agent (EXP-700 parent↔child messages). Best-effort and never throws; an old
 * relay 404s and reads as not-delivered, so callers must degrade with
 * guidance instead of assuming the message landed.
 */
export async function relayPostInput(
  config: SteerRelayConfig,
  sessionId: string,
  text: string,
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<{ delivered: boolean }> {
  try {
    const res = await fetchImpl(
      `${steerServerHttpBase(config)}/sessions/${encodeURIComponent(sessionId)}/input`,
      {
        method: `POST`,
        headers: {
          "content-type": `application/json`,
          "x-relay-secret": config.secret,
        },
        body: JSON.stringify({ text }),
        signal: AbortSignal.timeout(RELAY_INPUT_TIMEOUT_MS),
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
 * handler consumes it. `delivered: false` = the run has no live publisher,
 * the host never answered (older than the frame), or another ask is still
 * waiting — a tool ERROR on the web side, never a verdict; the four refusal
 * codes are the handler's own (`sessionsCompactRefusals`). */
export type RelayCompactOutcome =
  | { delivered: false; reason: `no_publisher` | `no_verdict` | `busy` | `relay_error` }
  | {
      delivered: true
      accepted: boolean
      refusedBecause?: `too_early` | `cooldown` | `not_own_session` | `unsupported_agent`
    }

/**
 * POST /sessions/:id/compact — relay the run's own `exponential_sessions_compact`
 * to its publisher and wait for the host's verdict (EXP-936). Never throws;
 * an old relay's 404 and a hung one both read as not-delivered with a
 * reason the handler can name.
 */
export async function relayPostCompact(
  config: SteerRelayConfig,
  sessionId: string,
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
        body: JSON.stringify(keep === undefined ? {} : { keep }),
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
          reason === `no_publisher` || reason === `no_verdict` || reason === `busy`
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
