// Steer relay — the outbound remote-start + live-terminal-steer hub
// (masterplan §3). Modeled on apps/push-relay (Hono HTTP, per-IP rate
// limiting, /healthz) with one structural difference: this is a stateful
// Bun-native WebSocket hub, so the default export carries a `websocket`
// handler beside `fetch`.
//
// The relay is a dumb pipe with auth + ephemeral presence: it verifies
// short-lived HS256 tickets (minted by the web app's `steer` tRPC router with
// the shared STEER_RELAY_SECRET), holds device presence + session rooms in
// memory, and never persists a byte. `STEER_RELAY_SECRET` unset ⇒ the relay
// refuses connections (503) — the web app equally treats the subsystem as off
// when STEER_RELAY_URL is unset.

import { timingSafeEqual } from "node:crypto"
import { Hono } from "hono"
import type { ServerWebSocket } from "bun"
import { verifySteerTicket, type SteerTicketClaims } from "@exp/steer-ticket"
import { Hub, type RelaySocket, type StartSubject } from "./hub"
import {
  CLOSE_UNAUTHORIZED,
  type StartRepoGroup,
  type StartSessionOptions,
} from "./protocol"

const RELAY_SECRET = process.env.STEER_RELAY_SECRET
if (!RELAY_SECRET) {
  console.warn(
    `[steer-relay] STEER_RELAY_SECRET not set — relay disabled (503 on all endpoints except /healthz)`
  )
}

export const hub = new Hub()

// ── Per-IP rate limiting (connects + admin calls; mirrors push-relay) ─────────

const RATE_LIMIT_MAX = 120
const RATE_LIMIT_WINDOW_MS = 60_000

interface RateBucket {
  count: number
  resetAt: number
}

const rateBuckets = new Map<string, RateBucket>()

function rateLimitHit(ip: string): boolean {
  const now = Date.now()
  const bucket = rateBuckets.get(ip)
  if (!bucket || bucket.resetAt <= now) {
    rateBuckets.set(ip, { count: 1, resetAt: now + RATE_LIMIT_WINDOW_MS })
    return true
  }
  bucket.count += 1
  return bucket.count <= RATE_LIMIT_MAX
}

setInterval(() => {
  const now = Date.now()
  for (const [ip, bucket] of rateBuckets) {
    if (bucket.resetAt <= now) rateBuckets.delete(ip)
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
    return c.json({ error: `Unauthorized` }, 401)
  }
  return next()
})

// Online desktops for a user — powers the phone's "Start on my desktop" picker.
app.get(`/devices/:userId`, (c) =>
  c.json({ devices: hub.devicesFor(c.req.param(`userId`)) })
)

// Liveness for a session room (the "is the desktop still publishing?" check).
app.get(`/sessions/:id`, (c) => c.json(hub.sessionInfo(c.req.param(`id`))))

// Remote "Start on my desktop": route to the device's control socket. The
// subject is EITHER a single issueId (wire-unchanged) or a batch group
// (issueIds + workspaceId + repo, all resolved server-side — the desktop syncs
// no repositories). Launch-option VALUES (EXP-149) and the batch fields pass
// through untouched — the web server already validated them, the relay stays a
// dumb pipe — but their TYPES/SHAPES are pinned here: a mistyped field would
// fail the desktop's serde parse and silently drop the whole frame after
// /start already answered ok.
app.post(`/start`, async (c) => {
  const body = (await c.req.json().catch(() => null)) as Record<
    string,
    unknown
  > | null
  const userId = asString(body?.userId)
  const deviceId = asString(body?.deviceId)
  if (!userId || !deviceId) {
    return c.json({ error: `Bad request` }, 400)
  }

  // Explicit XOR on key presence: single issueId, or the batch trio — never
  // both, never neither.
  const hasIssueId = body ? `issueId` in body : false
  const hasIssueIds = body ? `issueIds` in body : false
  let subject: StartSubject
  if (hasIssueId && !hasIssueIds) {
    const issueId = asString(body?.issueId)
    if (!issueId) return c.json({ error: `Bad request` }, 400)
    subject = { issueId }
  } else if (hasIssueIds && !hasIssueId) {
    const issueIds = asStringArray(body?.issueIds)
    const workspaceId = asString(body?.workspaceId)
    const repo = asStartRepo(body?.repo)
    if (!issueIds || !workspaceId || !repo) {
      return c.json({ error: `Bad request` }, 400)
    }
    subject = { issueIds, workspaceId, repo }
  } else {
    return c.json({ error: `Bad request` }, 400)
  }

  const options: StartSessionOptions = {
    model: asString(body?.model),
    effort: asString(body?.effort),
    ultracode: asBoolean(body?.ultracode),
    planMode: asBoolean(body?.planMode),
  }
  const result = hub.startSession(userId, deviceId, subject, options)
  if (!result.ok) return c.json({ error: result.reason }, 404)
  return c.json({ ok: true })
})

function asString(value: unknown): string | undefined {
  return typeof value === `string` ? value : undefined
}

function asBoolean(value: unknown): boolean | undefined {
  return typeof value === `boolean` ? value : undefined
}

// A batch issue-id array: 1..30 members, each a non-empty string ≤128 chars.
// Any deviation ⇒ undefined (⇒ 400 upstream).
function asStringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value) || value.length < 1 || value.length > 30) {
    return undefined
  }
  const out: string[] = []
  for (const entry of value) {
    if (typeof entry !== `string` || entry.length < 1 || entry.length > 128) {
      return undefined
    }
    out.push(entry)
  }
  return out
}

// The server-resolved batch repo group. REBUILDS the object so unknown keys
// (an installationId must never ride the frame) can't leak through.
function asStartRepo(value: unknown): StartRepoGroup | undefined {
  if (typeof value !== `object` || value === null) return undefined
  const obj = value as Record<string, unknown>
  const repositoryId = asString(obj.repositoryId)
  const fullName = asString(obj.fullName)
  const defaultBranch = asString(obj.defaultBranch)
  if (
    !repositoryId ||
    repositoryId.length > 128 ||
    !fullName ||
    fullName.length > 255 ||
    !defaultBranch ||
    defaultBranch.length > 255
  ) {
    return undefined
  }
  return { repositoryId, fullName, defaultBranch }
}

// Server-side kill-switch fallback (steer.killSession also flips the DB row).
app.post(`/sessions/:id/kill`, (c) => {
  const delivered = hub.killSession(c.req.param(`id`))
  return c.json({ ok: true, delivered })
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
      send: (data) => void ws.send(data),
      close: (code, reason) => ws.close(code, reason),
      bufferedAmount: () => ws.getBufferedAmount(),
    }
    adapters.set(ws, adapter)
  }
  return adapter
}

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
      if (!rateLimitHit(clientIp(req.headers))) {
        return new Response(`Rate limit exceeded`, { status: 429 })
      }
      const ticket = url.searchParams.get(`ticket`)
      const verdict = ticket
        ? verifySteerTicket(ticket, RELAY_SECRET)
        : ({ ok: false, reason: `malformed` } as const)
      if (!verdict.ok) {
        return new Response(`Unauthorized: ${verdict.reason}`, { status: 401 })
      }
      // Role allowlist: signature-valid tickets can still carry roles this
      // relay no longer serves (EXP-90 removed the anonymous `public_viewer`
      // audience) — a stale instance that still mints one gets 401, never a
      // socket.
      if (![`control`, `publisher`, `viewer`].includes(verdict.claims.role)) {
        return new Response(`Unauthorized: unknown_role`, { status: 401 })
      }
      const ok = server.upgrade(req, { data: { claims: verdict.claims } })
      return ok
        ? undefined
        : new Response(`Upgrade failed`, { status: 400 })
    }
    return app.fetch(req)
  },
  websocket: {
    // Keystrokes are tiny and output frames are chunked by the PTY; anything
    // bigger than this is abuse.
    maxPayloadLength: 1024 * 1024,
    open(ws: ServerWebSocket<WsData>) {
      hub.onOpen(adapt(ws), ws.data.claims)
    },
    message(ws: ServerWebSocket<WsData>, message: string | Buffer) {
      hub.onMessage(
        adapt(ws),
        typeof message === `string` ? message : new Uint8Array(message)
      )
    },
    close(ws: ServerWebSocket<WsData>) {
      hub.onClose(adapt(ws))
    },
  },
}

export { CLOSE_UNAUTHORIZED }
