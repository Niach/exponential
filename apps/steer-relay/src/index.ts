// Steer relay — device presence, control and eTalk calls (masterplan §3).
// Modeled on apps/push-relay (Hono HTTP, per-IP rate limiting, /healthz) with
// one structural difference: this is a stateful Bun-native WebSocket hub, so
// the default export carries a `websocket` handler beside `fetch`.
//
// It verifies short-lived HS256 tickets (minted by the web app with the
// shared STEER_RELAY_SECRET) and holds, in memory only, device presence (one
// CONTROL socket per device: `check_in` nudges, `kill` / `interrupt` /
// `compact_request` routed from the web server's admin calls) and NEW-55
// voice call rooms, whose binary Opus frames it forwards undecoded. VAPP-43:
// it carries NO session content: no session rooms, no activity, no history;
// viewers read sessions over eNet from the device (fixtures/steer-enet.json)
// and every content frame is refused. It never persists a byte.
// `STEER_RELAY_SECRET` unset ⇒ the relay refuses connections (503) — the web
// app equally treats the subsystem as off when STEER_RELAY_URL is unset.

import { timingSafeEqual } from "node:crypto"
import { Hono } from "hono"
import type { ServerWebSocket } from "bun"
import { verifySteerTicket, type SteerTicketClaims } from "@exp/steer-ticket"
import { Hub, type DeviceRef, type RelaySocket } from "./hub"
import { CLOSE_UNAUTHORIZED } from "./protocol"

const RELAY_SECRET = process.env.STEER_RELAY_SECRET
if (!RELAY_SECRET) {
  console.warn(
    `[steer-relay] STEER_RELAY_SECRET not set — relay disabled (503 on all endpoints except /healthz)`
  )
}

export const hub = new Hub()

// ── Per-IP rate limiting (WS upgrades + admin calls; valid/failed split) ──────

// Two buckets per IP, mirroring push-relay's failed-auth-only philosophy:
// failed auth (missing/invalid ticket, unknown role, wrong admin secret) gets a
// small brute-force budget, while ticket-VALID upgrades count against a
// separate, much larger one — so a garbage flood can never starve devices,
// and several teammates behind one NAT egress can't 429 each other, but a
// leaked-ticket replay flood still hits a ceiling. Without TRUST_PROXY every
// request keys to the shared `unknown` fallback, which makes the split
// load-bearing: one hostile client would otherwise drain the single global
// bucket for every user of the instance.
const RATE_LIMIT_FAILED_MAX = 120
const RATE_LIMIT_VALID_MAX = 1_200
const RATE_LIMIT_WINDOW_MS = 60_000

interface RateBucket {
  count: number
  resetAt: number
}

const rateBuckets = new Map<string, RateBucket>()

// EXP-553: monotonic 429 counter for /stats — bumped at every rate-limited
// rejection (HTTP middleware and the WS-upgrade paths alike).
let rateLimitedRejections = 0
const startedAt = Date.now()

function rateLimitHit(key: string, max: number): boolean {
  const now = Date.now()
  const bucket = rateBuckets.get(key)
  if (!bucket || bucket.resetAt <= now) {
    rateBuckets.set(key, { count: 1, resetAt: now + RATE_LIMIT_WINDOW_MS })
    return true
  }
  bucket.count += 1
  return bucket.count <= max
}

setInterval(() => {
  const now = Date.now()
  for (const [key, bucket] of rateBuckets) {
    if (bucket.resetAt <= now) rateBuckets.delete(key)
  }
}, RATE_LIMIT_WINDOW_MS).unref?.()

// Forwarded headers are client-forgeable: every spoofed value would mint its
// own fresh rate-limit bucket. They are honored only when TRUST_PROXY says a
// reverse proxy we control fronts the relay — and then only the RIGHTMOST
// x-forwarded-for entry (the one that proxy appended) counts. Otherwise all
// requests share the fallback bucket.
const TRUST_PROXY = process.env.TRUST_PROXY === `true`

// Loose IPv4/IPv6 shape check — anything else falls back to the shared bucket.
const IP_RE = /^(?:\d{1,3}(?:\.\d{1,3}){3}|[0-9a-fA-F:]+)$/

function clientIp(headers: Headers, fallback = `unknown`): string {
  if (!TRUST_PROXY) return fallback
  const forwarded = headers
    .get(`x-forwarded-for`)
    ?.split(`,`)
    .map((entry) => entry.trim())
    .filter(Boolean)
  const candidate = forwarded?.at(-1) ?? headers.get(`x-real-ip`)?.trim()
  if (candidate && IP_RE.test(candidate)) return candidate
  return fallback
}

// Constant-time secret check — a plain string compare leaks length and
// prefix-match timing on the single shared credential.
function secretMatches(provided: string | null): boolean {
  if (!provided || !RELAY_SECRET) return false
  const a = Buffer.from(provided)
  const b = Buffer.from(RELAY_SECRET)
  return a.length === b.length && timingSafeEqual(a, b)
}

// ── HTTP app (health + secret-authed server-to-server endpoints) ──────────────

const app = new Hono()

app.get(`/healthz`, (c) => c.json({ ok: true, ...hub.stats() }))

// Everything below requires the shared secret (web-server-to-relay only).
app.use(`*`, async (c, next) => {
  if (c.req.path === `/healthz`) return next()
  if (!RELAY_SECRET) return c.json({ error: `Relay not configured` }, 503)
  if (!secretMatches(c.req.raw.headers.get(`x-relay-secret`))) {
    // Only failed-auth attempts are throttled (REV2-65), sharing the upgrade
    // path's brute-force bucket. Secret-bearing traffic is trusted and never
    // throttled: every admin call arrives from the web server's single egress
    // IP, so a per-IP budget on it would drop legitimate admin calls.
    if (
      !rateLimitHit(
        `failed:${clientIp(c.req.raw.headers)}`,
        RATE_LIMIT_FAILED_MAX
      )
    ) {
      rateLimitedRejections += 1
      return c.json({ error: `Rate limit exceeded` }, 429)
    }
    return c.json({ error: `Unauthorized` }, 401)
  }
  return next()
})

// EXP-553: gauges + monotonic counters for the web app's admin performance
// page. Secret-gated by the middleware above (unlike the public /healthz,
// which deliberately stays gauges-only).
app.get(`/stats`, (c) =>
  c.json({
    ok: true,
    startedAt,
    ...hub.stats(),
    counters: { ...hub.counters(), rateLimitedRejections },
  })
)

// EXP-481: fire-and-forget check-in nudge — the web server persisted new
// work for the device (a queued command, edited launch defaults); an online
// device heartbeats immediately instead of waiting its cadence. Secret-gated
// by the middleware above like every relay endpoint; not rate-limited (single
// trusted web-server egress).
app.post(`/devices/:userId/:deviceId/nudge`, (c) =>
  c.json({
    ok: true,
    delivered: hub.nudge(c.req.param(`userId`), c.req.param(`deviceId`)),
  })
)

// VAPP-43: the session control routes name the DEVICE running the session in
// their JSON body — `{ userId, deviceId }`, userId being the account the
// device is registered under (the session's host) — and the relay sends the
// frame down that device's CONTROL socket. A body without both ids is a 400.
const ID_MAX_CHARS = 128

async function readDeviceBody(c: {
  req: { json(): Promise<unknown> }
}): Promise<{ device: DeviceRef; body: Record<string, unknown> } | null> {
  const body = (await c.req.json().catch(() => null)) as Record<
    string,
    unknown
  > | null
  if (!body || typeof body !== `object`) return null
  const { userId, deviceId } = body
  if (
    typeof userId !== `string` ||
    typeof deviceId !== `string` ||
    userId.length === 0 ||
    deviceId.length === 0 ||
    userId.length > ID_MAX_CHARS ||
    deviceId.length > ID_MAX_CHARS
  ) {
    return null
  }
  return { device: { userId, deviceId }, body }
}

// Server-side kill (every end the server makes; the DB row flip is the
// durable path). 200 with delivered=false when the device is not connected.
app.post(`/sessions/:id/kill`, async (c) => {
  const parsed = await readDeviceBody(c)
  if (!parsed) return c.json({ error: `Bad request` }, 400)
  const delivered = hub.killSession(c.req.param(`id`), parsed.device)
  return c.json({ ok: true, delivered })
})

// EXP-1291: `sessions.stop` — interrupt the running turn. Same delivery
// contract as kill.
app.post(`/sessions/:id/interrupt`, async (c) => {
  const parsed = await readDeviceBody(c)
  if (!parsed) return c.json({ error: `Bad request` }, 400)
  const delivered = hub.interruptSession(c.req.param(`id`), parsed.device)
  return c.json({ ok: true, delivered })
})

// EXP-936: the run's own `exponential_sessions_compact`, relayed down its
// device's control socket as a `compact_request` and AWAITED: the host
// answers with its verdict (`compact_verdict`) and that answer is this
// response. `keep` mirrors the tool's argument (what the summary must
// preserve). 200 with delivered=false when the device is not connected
// (`device_offline`), the host never answered (`no_verdict`) or another ask
// for the session waits (`busy`).
const COMPACT_KEEP_MAX_CHARS = 2_000

app.post(`/sessions/:id/compact`, async (c) => {
  const parsed = await readDeviceBody(c)
  if (!parsed) return c.json({ error: `Bad request` }, 400)
  const keep = parsed.body.keep
  if (
    keep !== undefined &&
    (typeof keep !== `string` || keep.length === 0 || keep.length > COMPACT_KEEP_MAX_CHARS)
  ) {
    return c.json({ error: `Bad request` }, 400)
  }
  const outcome = await hub.requestCompaction(
    c.req.param(`id`),
    parsed.device,
    keep
  )
  return c.json({ ok: true, ...outcome })
})

// ── WebSocket upgrade + handlers ──────────────────────────────────────────────

interface WsData {
  claims: SteerTicketClaims
}

// ServerWebSocket → the hub's testable socket interface.
const adapters = new WeakMap<ServerWebSocket<WsData>, RelaySocket>()

function adapt(ws: ServerWebSocket<WsData>): RelaySocket {
  let adapter = adapters.get(ws)
  if (!adapter) {
    adapter = {
      send: (data: string) => void ws.send(data),
      // NEW-55: a Uint8Array goes out as a BINARY frame.
      sendBinary: (data: Uint8Array) => void ws.send(data),
      close: (code, reason) => ws.close(code, reason),
      bufferedAmount: () => ws.getBufferedAmount(),
    }
    adapters.set(ws, adapter)
  }
  return adapter
}

/** The ticket roles the relay opens a socket for: the device's presence and
 *  control socket, and NEW-55's voice call pair. */
export const ACCEPTED_ROLES = [`control`, `caller`, `call_host`] as const

const port = parseInt(process.env.PORT ?? `4002`, 10)
console.log(`[steer-relay] listening on :${port}`)

export default {
  port,
  fetch(req: Request, server: { upgrade(req: Request, opts: { data: WsData }): boolean }) {
    const url = new URL(req.url)
    if (url.pathname === `/ws`) {
      if (!RELAY_SECRET) {
        return new Response(`Relay not configured`, { status: 503 })
      }
      // Ticket verification runs BEFORE any rate accounting: HS256 verify is
      // cheap, and which bucket a connect belongs to depends on the verdict —
      // counting unauthenticated garbage against the same budget as valid
      // tickets would let one hostile client 429 every legitimate user.
      const ticket = url.searchParams.get(`ticket`)
      const verdict = ticket
        ? verifySteerTicket(ticket, RELAY_SECRET)
        : ({ ok: false, reason: `malformed` } as const)
      if (!verdict.ok) {
        if (!rateLimitHit(`failed:${clientIp(req.headers)}`, RATE_LIMIT_FAILED_MAX)) {
          rateLimitedRejections += 1
          return new Response(`Rate limit exceeded`, { status: 429 })
        }
        return new Response(`Unauthorized: ${verdict.reason}`, { status: 401 })
      }
      // Role allowlist: signature-valid tickets can still carry roles this
      // relay no longer serves (EXP-90 removed the anonymous `public_viewer`
      // audience; VAPP-43 removed `publisher` and `viewer`: session content
      // goes over eNet) — a stale instance or old build that still mints one
      // gets 401, never a socket. Counts as failed auth.
      if (!(ACCEPTED_ROLES as readonly string[]).includes(verdict.claims.role)) {
        if (!rateLimitHit(`failed:${clientIp(req.headers)}`, RATE_LIMIT_FAILED_MAX)) {
          rateLimitedRejections += 1
          return new Response(`Rate limit exceeded`, { status: 429 })
        }
        return new Response(`Unauthorized: unknown_role`, { status: 401 })
      }
      if (!rateLimitHit(`valid:${clientIp(req.headers)}`, RATE_LIMIT_VALID_MAX)) {
        rateLimitedRejections += 1
        return new Response(`Rate limit exceeded`, { status: 429 })
      }
      const ok = server.upgrade(req, { data: { claims: verdict.claims } })
      return ok
        ? undefined
        : new Response(`Upgrade failed`, { status: 400 })
    }
    return app.fetch(req)
  },
  websocket: {
    // The biggest legitimate text frame is a `call_event` (8 KiB), but an
    // old build's control socket may still send a large content frame: it
    // must be answered `content_refused`, not have its socket closed by a
    // payload cap, so the cap stays where the content protocol had it.
    maxPayloadLength: 1024 * 1024,
    open(ws: ServerWebSocket<WsData>) {
      hub.onOpen(adapt(ws), ws.data.claims)
    },
    // Text frames are the hub's JSON. NEW-55: binary frames (Bun hands them
    // over as a Buffer) go to `onBinary`, which forwards only a paired call
    // room's audio and drops a binary frame from every other role, exactly
    // as this handler used to.
    message(ws: ServerWebSocket<WsData>, message: string | Buffer) {
      if (typeof message === `string`) {
        hub.onMessage(adapt(ws), message)
        return
      }
      hub.onBinary(adapt(ws), message)
    },
    close(ws: ServerWebSocket<WsData>) {
      hub.onClose(adapt(ws))
    },
  },
}

export { CLOSE_UNAUTHORIZED }
