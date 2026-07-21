import { timingSafeEqual } from "node:crypto"
import { Hono } from "hono"
import { cert, getApps, initializeApp, type App } from "firebase-admin/app"
import { getMessaging } from "firebase-admin/messaging"
import { z } from "zod"

// ── Relay auth (mandatory) ────────────────────────────────────────────────────

// The secret is REQUIRED: without it the relay would be an open endpoint that
// lets anyone on the internet push operator-signed notifications to harvested
// device tokens. Failing to start is the only safe posture.
const RELAY_SECRET = requireRelaySecret()

function requireRelaySecret(): string {
  const secret = process.env.PUSH_RELAY_SECRET
  if (!secret) {
    console.error(
      `[push-relay] PUSH_RELAY_SECRET is not set — refusing to start. Set the same secret on this process and on the web app.`
    )
    process.exit(1)
  }
  return secret
}

function secretMatches(provided: string | null): boolean {
  if (!provided) return false
  const a = Buffer.from(provided)
  const b = Buffer.from(RELAY_SECRET)
  return a.length === b.length && timingSafeEqual(a, b)
}

// ── Firebase init (lazy singleton) ───────────────────────────────────────────

let firebaseApp: App | null | undefined

function getFirebaseApp(): App | null {
  if (firebaseApp !== undefined) return firebaseApp

  const raw = process.env.FIREBASE_SERVICE_ACCOUNT_JSON
  if (!raw) {
    console.warn(`[push-relay] FIREBASE_SERVICE_ACCOUNT_JSON not set — relay disabled`)
    firebaseApp = null
    return firebaseApp
  }
  try {
    const creds = JSON.parse(raw)
    firebaseApp =
      getApps()[0] ??
      initializeApp({
        credential: cert({
          projectId: creds.project_id,
          clientEmail: creds.client_email,
          privateKey: creds.private_key,
        }),
      })
    return firebaseApp
  } catch (err) {
    console.error(`[push-relay] Failed to init firebase-admin:`, err)
    firebaseApp = null
    return firebaseApp
  }
}

// ── Validation ────────────────────────────────────────────────────────────────

const sendSchema = z.object({
  tokens: z.array(z.string().min(1)).min(1).max(500),
  notification: z.object({
    title: z.string().min(1),
    body: z.string().optional(),
  }),
  data: z.record(z.string(), z.string()),
})

// ── Per-IP rate limiting (failed-auth attempts only) ─────────────────────────

const RATE_LIMIT_MAX = 60
const RATE_LIMIT_WINDOW_MS = 60_000
const MAX_BODY_BYTES = 64 * 1024

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

// Best-effort sweep of the map so it can't grow unbounded under traffic.
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

// Read a request body with a hard byte cap, aborting the stream as soon as it
// exceeds the cap. Returns null when over the cap; a read error yields an
// empty string (which then fails JSON parsing as before).
async function readBodyCapped(
  body: ReadableStream<Uint8Array> | null,
  maxBytes: number
): Promise<string | null> {
  if (!body) return ``
  const reader = body.getReader()
  const chunks: Uint8Array[] = []
  let total = 0
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      total += value.byteLength
      if (total > maxBytes) {
        await reader.cancel().catch(() => {})
        return null
      }
      chunks.push(value)
    }
  } catch {
    return ``
  }
  return Buffer.concat(chunks).toString(`utf8`)
}

// ── Dead-token error codes ────────────────────────────────────────────────────
// Codes the FCM v1 path (sendEachForMulticast) can actually emit for a token
// that is PERMANENTLY undeliverable and must be pruned:
//   - registration-token-not-registered: v1 UNREGISTERED, the canonical
//     dead-token code.
//   - mismatched-credential: v1 SENDER_ID_MISMATCH — the token belongs to a
//     different Firebase project and will never work with our credential.
// Deliberately absent: invalid-argument (also fired for malformed payloads —
// pruning on it would mass-delete valid tokens on a payload bug) and
// invalid-registration-token (legacy-API-only; the v1 endpoint never maps to
// it, so listing it was dead weight).

const DEAD_CODES = new Set([
  `messaging/registration-token-not-registered`,
  `messaging/mismatched-credential`,
])

// ── FCM deadline ──────────────────────────────────────────────────────────────

// firebase-admin exposes no per-call timeout/abort, so slow FCM would hold
// every /send response — and the web app's fetch-pool slot behind it — open
// indefinitely. The web app aborts its POST after 10s (REV2-3); answering 504
// just under that keeps the failure visible to the caller instead of an abort.
// The orphaned multicast keeps settling in the background (its sends may still
// deliver); only that batch's invalid-token pruning is lost, which the next
// successful send or the web app's stale-token sweep recovers.
const FCM_DEADLINE_MS = 8_000

class DeadlineError extends Error {}

function withDeadline<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new DeadlineError(`deadline of ${ms}ms exceeded`)),
      ms
    )
    promise.then(
      (value) => {
        clearTimeout(timer)
        resolve(value)
      },
      (err) => {
        clearTimeout(timer)
        reject(err)
      }
    )
  })
}

// ── Hono app ──────────────────────────────────────────────────────────────────

const app = new Hono()

// Unauthenticated health check for Docker HEALTHCHECK / uptime monitors
app.get(`/healthz`, (c) => c.json({ ok: true }))

app.post(`/send`, async (c) => {
  if (!secretMatches(c.req.raw.headers.get(`x-relay-secret`))) {
    // Only failed-auth attempts are rate limited (a brute-force throttle).
    // Secret-bearing traffic is trusted and never throttled: the web app fans
    // out one POST per push recipient from a single egress IP, so any per-IP
    // budget would silently drop legitimate pushes on busy issues.
    if (!rateLimitHit(clientIp(c.req.raw.headers))) {
      return c.json({ error: `Rate limit exceeded` }, 429)
    }
    return c.json({ error: `Unauthorized` }, 401)
  }

  // Reject oversized bodies before parsing JSON — the relay only ever needs
  // tokens + a short notification. Cheap pre-check on the declared length…
  const contentLength = Number.parseInt(
    c.req.raw.headers.get(`content-length`) ?? `0`,
    10
  )
  if (contentLength > MAX_BODY_BYTES) {
    return c.json({ error: `Body too large` }, 413)
  }

  const firebase = getFirebaseApp()
  if (!firebase) {
    return c.json({ error: `Firebase not configured` }, 503)
  }

  // …then a size-capped streaming read: a chunked request carries no
  // Content-Length, so reading the stream and aborting past the cap is the
  // only way to avoid buffering an unbounded body into memory.
  const rawText = await readBodyCapped(c.req.raw.body, MAX_BODY_BYTES)
  if (rawText === null) {
    return c.json({ error: `Body too large` }, 413)
  }
  const body = (() => {
    try {
      return JSON.parse(rawText) as unknown
    } catch {
      return null
    }
  })()
  const parsed = sendSchema.safeParse(body)
  if (!parsed.success) {
    return c.json({ error: `Bad request`, issues: parsed.error.issues }, 400)
  }

  const { tokens, notification, data } = parsed.data
  const messaging = getMessaging(firebase)

  let response
  try {
    response = await withDeadline(
      messaging.sendEachForMulticast({
        tokens,
        notification: { title: notification.title, body: notification.body },
        data,
        android: {
          priority: `high`,
          notification: { channelId: `issues_default` },
        },
        apns: {
          headers: { "apns-priority": `10` },
          payload: {
            aps: {
              alert: { title: notification.title, body: notification.body },
              sound: `default`,
              contentAvailable: true,
            },
          },
        },
      }),
      FCM_DEADLINE_MS
    )
  } catch (err) {
    if (err instanceof DeadlineError) {
      console.error(
        `[push-relay] FCM multicast exceeded ${FCM_DEADLINE_MS}ms deadline (${tokens.length} tokens)`
      )
      return c.json({ error: `FCM timeout` }, 504)
    }
    console.error(`[push-relay] FCM multicast failed:`, err)
    return c.json({ error: `FCM error` }, 500)
  }

  const invalidTokens: string[] = []
  response.responses.forEach((res, i) => {
    if (res.success) return
    const code = res.error?.code
    if (code && DEAD_CODES.has(code)) {
      invalidTokens.push(tokens[i])
    } else {
      console.error(
        `[push-relay] send error token=${tokens[i]?.slice(0, 12)}… code=${res.error?.code ?? `unknown`}`
      )
    }
  })

  return c.json({ invalidTokens })
})

// ── Start ─────────────────────────────────────────────────────────────────────

const port = parseInt(process.env.PORT ?? `4001`, 10)
console.log(`[push-relay] listening on :${port}`)

export default {
  port,
  fetch: app.fetch,
}
