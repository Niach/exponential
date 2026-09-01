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
  type StartInput,
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

// ── Per-IP rate limiting (WS upgrades + admin calls; valid/failed split) ──────

// Two buckets per IP, mirroring push-relay's failed-auth-only philosophy:
// failed auth (missing/invalid ticket, unknown role, wrong admin secret) gets a
// small brute-force budget, while ticket-VALID upgrades count against a
// separate, much larger one — so a garbage flood can never starve viewers,
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
    // IP, so a per-IP budget on it would drop legitimate remote starts.
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
// which deliberately stays gauges-only); an old web app simply never calls
// this, and an old relay 404s — the caller falls back to /healthz.
app.get(`/stats`, (c) =>
  c.json({
    ok: true,
    startedAt,
    ...hub.stats(),
    counters: { ...hub.counters(), rateLimitedRejections },
  })
)

// Online desktops for a user — powers the phone's "Start on my desktop" picker.
app.get(`/devices/:userId`, (c) =>
  c.json({ devices: hub.devicesFor(c.req.param(`userId`)) })
)

// Liveness for a session room (the "is the desktop still publishing?" check).
app.get(`/sessions/:id`, (c) => c.json(hub.sessionInfo(c.req.param(`id`))))

// Remote "Start on my desktop": route to the device's control socket. The
// subject is a single issueId (wire-unchanged), a batch group (issueIds +
// teamId + repo), or an action run (actionId + actionName + teamId +
// optional repo — EXP-253); all resolved server-side — the desktop syncs
// no repositories. Launch-option VALUES (EXP-149) and the subject fields pass
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

  // Explicit exactly-one on key presence: single issueId, the batch trio, or
  // the action subject — never more than one, never none.
  // EXP-637 resume: the ONLY subject that may carry issueId/actionId
  // alongside itself (they are display hints, not the subject), so it is
  // checked before the exactly-one count.
  const hasResume = body ? `resumeSessionId` in body : false
  const hasIssueId = body ? `issueId` in body : false
  const hasIssueIds = body ? `issueIds` in body : false
  const hasActionId = body ? `actionId` in body : false
  const subjectCount = [hasIssueId, hasIssueIds, hasActionId].filter(
    Boolean
  ).length
  let subject: StartSubject
  if (hasResume) {
    const resumeSessionId = asString(body?.resumeSessionId)
    const teamId = asString(body?.teamId)
    if (
      !resumeSessionId ||
      resumeSessionId.length > 128 ||
      !teamId ||
      teamId.length > 128 ||
      hasIssueIds
    ) {
      return c.json({ error: `Bad request` }, 400)
    }
    // Same stance as repo/inputs: the hint keys are OPTIONAL, but a PRESENT
    // key must parse — a mistyped field would drop the whole frame desktop
    // side after /start already answered ok.
    let issueId: string | undefined
    if (body && `issueId` in body) {
      issueId = asString(body.issueId)
      if (!issueId || issueId.length > 128) {
        return c.json({ error: `Bad request` }, 400)
      }
    }
    let actionId: string | undefined
    if (body && `actionId` in body) {
      actionId = asString(body.actionId)
      if (!actionId || actionId.length > 128) {
        return c.json({ error: `Bad request` }, 400)
      }
    }
    let actionName: string | undefined
    if (body && `actionName` in body) {
      actionName = asString(body.actionName)
      if (!actionName || actionName.length > 255) {
        return c.json({ error: `Bad request` }, 400)
      }
    }
    let branch: string | undefined
    if (body && `branch` in body) {
      branch = asString(body.branch)
      if (!branch || branch.length > 255) {
        return c.json({ error: `Bad request` }, 400)
      }
    }
    subject = {
      resumeSessionId,
      teamId,
      ...(issueId ? { issueId } : {}),
      ...(actionId ? { actionId } : {}),
      ...(actionName ? { actionName } : {}),
      ...(branch ? { branch } : {}),
    }
  } else if (subjectCount !== 1) {
    return c.json({ error: `Bad request` }, 400)
  } else if (hasIssueId) {
    const issueId = asString(body?.issueId)
    if (!issueId) return c.json({ error: `Bad request` }, 400)
    subject = { issueId }
  } else if (hasActionId) {
    const actionId = asString(body?.actionId)
    const actionName = asString(body?.actionName)
    const teamId = asString(body?.teamId)
    if (
      !actionId ||
      actionId.length > 128 ||
      !actionName ||
      actionName.length > 255 ||
      !teamId ||
      teamId.length > 128
    ) {
      return c.json({ error: `Bad request` }, 400)
    }
    // repo is OPTIONAL (repo-less actions) — but a PRESENT repo key must
    // parse, else 400 (a malformed repo would drop the frame desktop-side).
    let repo: StartRepoGroup | undefined
    if (body && `repo` in body) {
      repo = asStartRepo(body.repo)
      if (!repo) return c.json({ error: `Bad request` }, 400)
    }
    // inputs are OPTIONAL (EXP-257) — same stance: a present key must parse.
    let inputs: StartInput[] | undefined
    if (body && `inputs` in body) {
      inputs = asStartInputs(body.inputs)
      if (!inputs) return c.json({ error: `Bad request` }, 400)
    }
    subject = {
      actionId,
      actionName,
      teamId,
      ...(repo ? { repo } : {}),
      ...(inputs ? { inputs } : {}),
    }
  } else {
    const issueIds = asStringArray(body?.issueIds)
    const teamId = asString(body?.teamId)
    const repo = asStartRepo(body?.repo)
    if (!issueIds || !teamId || !repo) {
      return c.json({ error: `Bad request` }, 400)
    }
    subject = { issueIds, teamId, repo }
  }

  // startedBy is OPTIONAL (EXP-432: requester attribution on a shared-device
  // start) — but a PRESENT key must be a sane string, else 400 (same stance
  // as repo/inputs: a mistyped field would drop the frame desktop-side after
  // /start already answered ok).
  let startedBy: string | undefined
  if (body && `startedBy` in body) {
    startedBy = asString(body.startedBy)
    if (!startedBy || startedBy.length > 128) {
      return c.json({ error: `Bad request` }, 400)
    }
  }

  // startedReason is OPTIONAL (EXP-679: this start was asked for by another
  // coding session) — a PRESENT key must be exactly `agent`, the one value
  // the wire carries; anything else is a 400, same stance as startedBy.
  let startedReason: `agent` | undefined
  if (body && `startedReason` in body) {
    if (body.startedReason !== `agent`) {
      return c.json({ error: `Bad request` }, 400)
    }
    startedReason = `agent`
  }

  const options: StartSessionOptions = {
    ...(startedBy ? { startedBy } : {}),
    ...(startedReason ? { startedReason } : {}),
    agent: asString(body?.agent),
    model: asString(body?.model),
    effort: asString(body?.effort),
    ultracode: asBoolean(body?.ultracode),
    planMode: asBoolean(body?.planMode),
    // EXP-690: a retired `skipPermissions` in the body is read by nobody —
    // old callers keep sending it and the frame simply never carries it.
    resume: asBoolean(body?.resume),
  }
  const result = hub.startSession(userId, deviceId, subject, options)
  if (!result.ok) return c.json({ error: result.reason }, 404)
  return c.json({ ok: true })
})

// EXP-481: fire-and-forget check-in nudge — the web server persisted new
// work for the device (a queued command, edited launch defaults); an online
// device heartbeats immediately instead of waiting its cadence. Secret-gated
// by the middleware above like every relay endpoint; not rate-limited (single
// trusted web-server egress, same stance as /start).
app.post(`/devices/:userId/:deviceId/nudge`, (c) =>
  c.json({
    ok: true,
    delivered: hub.nudge(c.req.param(`userId`), c.req.param(`deviceId`)),
  })
)

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

// The resolved action-input values (EXP-257). REBUILDS each entry so unknown
// keys can't leak through. Bounds: ≤10 entries; key ≤64, label ≤255, type
// ≤16, value/display ≤16KB (a 4096-char text value is ≤16KB in UTF-8). Any
// deviation ⇒ undefined (⇒ 400 upstream).
function asStartInputs(value: unknown): StartInput[] | undefined {
  if (!Array.isArray(value) || value.length > 10) return undefined
  const out: StartInput[] = []
  for (const entry of value) {
    if (typeof entry !== `object` || entry === null) return undefined
    const obj = entry as Record<string, unknown>
    const key = asString(obj.key)
    const label = asString(obj.label)
    const type = asString(obj.type)
    const val = asString(obj.value)
    const display = asString(obj.display)
    if (
      !key ||
      key.length > 64 ||
      !label ||
      label.length > 255 ||
      !type ||
      type.length > 16 ||
      val === undefined ||
      val.length > 16 * 1024 ||
      display === undefined ||
      display.length > 16 * 1024
    ) {
      return undefined
    }
    out.push({ key, label, type, value: val, display })
  }
  return out
}

// Server-side kill-switch fallback (steer.killSession also flips the DB row).
app.post(`/sessions/:id/kill`, (c) => {
  const delivered = hub.killSession(c.req.param(`id`))
  return c.json({ ok: true, delivered })
})

// EXP-700: inject text as the session owner's input (parent↔child messages —
// a child's question/report into its parent, a parent's answer into its
// child). Mirrors the kill contract: 200 with delivered=false when no live
// publisher holds the room.
const INPUT_TEXT_MAX_CHARS = 16 * 1024

app.post(`/sessions/:id/input`, async (c) => {
  const body = (await c.req.json().catch(() => null)) as Record<
    string,
    unknown
  > | null
  const text = typeof body?.text === `string` ? body.text : ``
  if (text.length === 0 || text.length > INPUT_TEXT_MAX_CHARS) {
    return c.json({ error: `Bad request` }, 400)
  }
  const delivered = hub.injectInput(c.req.param(`id`), text)
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
      send: (data: string) => void ws.send(data),
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
      // audience) — a stale instance that still mints one gets 401, never a
      // socket. Counts as failed auth: it never gets a socket either.
      if (![`control`, `publisher`, `viewer`].includes(verdict.claims.role)) {
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
    // Keystrokes are tiny; the biggest legitimate frame is an activity event
    // carrying a worktree diff (schema-capped at 512KB). Anything bigger than
    // this is abuse.
    maxPayloadLength: 1024 * 1024,
    open(ws: ServerWebSocket<WsData>) {
      hub.onOpen(adapt(ws), ws.data.claims)
    },
    // Text frames only: the hub speaks JSON. Anything binary is dropped here
    // rather than handed down.
    message(ws: ServerWebSocket<WsData>, message: string | Buffer) {
      if (typeof message !== `string`) return
      hub.onMessage(adapt(ws), message)
    },
    // REV2-X: Bun routes protocol-level ping frames HERE, not to `message`.
    // The desktop publisher pings every 30s during idle/plan-mode; without
    // this the idle detector would never see them and would detach a
    // live-but-quiet publisher after 90s (a churny reconnect — EXP-283 made
    // the idle close non-terminal).
    ping(ws: ServerWebSocket<WsData>) {
      hub.onPing(adapt(ws))
    },
    close(ws: ServerWebSocket<WsData>) {
      hub.onClose(adapt(ws))
    },
  },
}

export { CLOSE_UNAUTHORIZED }
