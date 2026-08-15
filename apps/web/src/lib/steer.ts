// Steer relay helpers — the pure core of the `steer` tRPC router (masterplan
// §3.5). Ticket-claim composition, relay URL derivation, and the
// secret-authed server-to-server relay HTTP calls live here so they are
// unit-testable without a DB or a live relay. The wire truth for everything
// steer is apps/steer-relay/src/protocol.ts + the ticket format in
// packages/steer-ticket.

import { signSteerTicket, type SteerTicketClaims } from "@exp/steer-ticket"
import type { SteerStartInput } from "@/lib/action-inputs"

export type { SteerStartInput } from "@/lib/action-inputs"

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
// to its session; the old `perm` claim is gone from the wire (clean cut —
// bump CLIENT_MIN_VERSION_* past pre-EXP-312 builds when deploying).
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
        ...(seed.deviceLabel ? { deviceLabel: seed.deviceLabel } : {}),
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
    signal?: AbortSignal
  }
) => Promise<RelayResponse>

export interface SteerDevice {
  deviceId: string
  deviceLabel: string
  connectedAt: number
  /** EXP-201: agent CLIs the device advertised (`claude`/`codex`/`pi`).
   * Absent from an old relay ⇒ treat as claude-only. Since EXP-409:
   * RUNNABLE (installed AND signed in) — explicitly empty means the
   * machine can run nothing right now. */
  agents?: string[]
  /** EXP-409: agents installed but SIGNED OUT on the device — unusable;
   * surfaced so UIs can say "sign in on that machine". Absent (old
   * relay/sender) ⇒ none known. */
  unauthedAgents?: string[]
  /** EXP-253: feature capabilities the device advertised (`actions`, and
   * since EXP-257 `action-inputs` — required for builtin or inputs-carrying
   * action starts). Absent (old desktop/relay) ⇒ none — action starts are
   * strictly gated on this, unlike the lenient agents fallback. */
  caps?: string[]
  /** EXP-437: the machine's per-agent launch defaults, passed through
   * verbatim from its online frame — remote Start-coding dialogs seed
   * their options from the selected device. Absent (old desktop/relay) ⇒
   * clients seed static contract defaults. */
  launchDefaults?: SteerLaunchDefaults
}

/** EXP-437: one agent's launch defaults on a device. Blank `model`/`effort`
 * = "CLI default / omit the flag"; absent booleans = false. */
export interface SteerAgentLaunchDefaults {
  model?: string
  effort?: string
  ultracode?: boolean
  planMode?: boolean
  skipPermissions?: boolean
}

/** EXP-437: a device's launch-defaults advertisement — `agents` keyed by
 * contract `codingAgent` id, covering only that machine's RUNNABLE agents. */
export interface SteerLaunchDefaults {
  defaultAgent?: string
  agents?: Record<string, SteerAgentLaunchDefaults>
}

/**
 * EXP-414: `devices.list` calls this on every agents-page poll and paints the
 * result raw — a wedged relay must fail fast into the caller's offline
 * fallback, not hold the whole device list open.
 */
const RELAY_DEVICES_TIMEOUT_MS = 3_000

/** GET /devices/:userId — online desktops for the "Start on my desktop" picker. */
export async function relayGetDevices(
  config: SteerRelayConfig,
  userId: string,
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<{ devices: SteerDevice[] }> {
  const res = await fetchImpl(
    `${steerServerHttpBase(config)}/devices/${encodeURIComponent(userId)}`,
    {
      headers: { "x-relay-secret": config.secret },
      signal: AbortSignal.timeout(RELAY_DEVICES_TIMEOUT_MS),
    }
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
  /** EXP-481: resume the issue's existing worktree/agent session instead of
   * starting fresh. Single-issue starts only; the web server gates it on the
   * device's `resume` cap, and the device-side launcher degrades a missing/
   * foreign worktree to a fresh session seeded with a resume prompt. */
  resume?: boolean
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
 * The subject of a remote start: a single issue (wire-unchanged), a batch of
 * issues sharing one team + repo group, or an action (EXP-253 — the name is
 * a display snapshot so the desktop can title the tab/trust dialog before
 * its own `actions.get` resolves; `repo` is absent for repo-less actions).
 * `inputs` (EXP-257) are the action's filled input values, fully resolved
 * server-side (display names included) so the desktop injects them into the
 * prompt with zero lookups. Exactly one form.
 */
export type SteerStartSubject =
  | { issueId: string }
  | { issueIds: string[]; teamId: string; repo: SteerStartRepo }
  | {
      actionId: string
      actionName: string
      teamId: string
      repo?: SteerStartRepo
      inputs?: SteerStartInput[]
    }

/**
 * REV-34: `steer.startSession` awaits this call inline — an accepting-but-
 * wedged relay must fail the Start button fast, not pin the mutation (and a
 * server connection) open until the client gives up. A healthy relay answers
 * /start in milliseconds (it validates and routes before replying), so 3s is
 * generous; the timeout resolves into the structured failure shape below and
 * the caller surfaces it as a retryable relay error.
 */
const RELAY_START_TIMEOUT_MS = 3_000

/** POST /start — route a remote start to the device's control socket.
 * Undefined option fields are dropped by JSON.stringify — never sent.
 * `startedBy` (EXP-432): the requesting teammate on a start targeting a
 * SHARED server device — `userId` is then the device OWNER (whose presence
 * bucket holds the socket). Absent on every own-device start. */
export async function relayPostStart(
  config: SteerRelayConfig,
  body: { userId: string; deviceId: string; startedBy?: string } &
    SteerStartSubject &
    SteerStartOptions,
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<RelayStartResult> {
  let res: RelayResponse
  try {
    res = await fetchImpl(`${steerServerHttpBase(config)}/start`, {
      method: `POST`,
      headers: {
        "content-type": `application/json`,
        "x-relay-secret": config.secret,
      },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(RELAY_START_TIMEOUT_MS),
    })
  } catch (err) {
    // The armed timeout raises a `TimeoutError` DOMException — which does not
    // extend Error in every runtime, so match on the name alone.
    if ((err as { name?: unknown } | null)?.name === `TimeoutError`) {
      return { ok: false, status: 504, reason: `relay_timeout` }
    }
    throw err
  }
  if (res.ok) return { ok: true }
  const json = (await res.json().catch(() => null)) as {
    error?: string
  } | null
  return { ok: false, status: res.status, reason: json?.error ?? `relay_error` }
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
