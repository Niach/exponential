import { timingSafeEqual } from "node:crypto"
import { Hono, type Context } from "hono"
import { mountCompatSend, type CompatSendStats } from "./compat-send"
import { consoleLogger, type Logger } from "./log"
import { mountPushApi, type PushApiOptions } from "./push-api"
import {
  DeadlineError,
  FCM_DEADLINE_MS,
  clientIpFrom,
  createIpLimiter,
  withDeadline,
  type RelayMessaging,
} from "./shared"

export { DeadlineError, FCM_DEADLINE_MS, type RelayMessaging } from "./shared"
export { TRANSPORT_ERROR_CODE, isTransportError } from "./compat-send"

// The relay's HTTP surface, built by `createRelayApp` so the tests can drive
// it with fakes (messaging, APNs, an in-memory store, a clock). `index.ts`
// wires the real environment.
//
//   push API (push-api.ts)        /installs…, /push: the ONE public API
//   compat shim (compat-send.ts)  /send: deprecated, PUSH_RELAY_SECRET only
//   operator routes (here)        /healthz (public), /healthz?deep=1 and
//                                 /stats (PUSH_RELAY_SECRET)

/** The push API's own configuration; the relay injects the clock, log, FCM client and auth throttle. */
export type PushApiConfig = Omit<
  PushApiOptions,
  `getMessaging` | `now` | `log` | `clientIp` | `failedAuthHit` | `fcmDeadlineMs`
>

export interface RelayAppOptions {
  /**
   * `PUSH_RELAY_SECRET`: the operator secret, shared with the web app. It
   * gates the /send compat shim and the operator routes (/stats,
   * /healthz?deep=1). Null = all three answer 404; the push API is unaffected.
   */
  secret: string | null
  /** `TRUST_PROXY=true`: honor the reverse proxy's forwarded client address. */
  trustProxy: boolean
  /** The firebase-admin messaging client, or null while Firebase is not configured. */
  getMessaging: () => RelayMessaging | null
  /** The push API: always mounted (the relay does not start without its store). */
  api: PushApiConfig
  /** Budget for one provider call, deadline included (see FCM_DEADLINE_MS). */
  fcmDeadlineMs?: number
  now?: () => number
  log?: Logger
}

// ── Per-IP rate limiting (failed-auth attempts only) ─────────────────────────

const RATE_LIMIT_MAX = 60
const RATE_LIMIT_WINDOW_MS = 60_000

// ── Deep health check (EXP-1289) ──────────────────────────────────────────────

// `/healthz?deep=1` (secret-gated) sends a DRY-RUN multicast to a token that
// cannot exist. FCM validates the request and answers with a per-token
// result — the transport (auth token, HTTP/2 session, TLS) is exercised end to
// end and no device is touched. Any settled per-token result means the
// transport works; only a thrown session error or the deadline means it does
// not. The plain `/healthz` stays the constant Docker's HEALTHCHECK polls.
export const DEEP_CHECK_TOKEN = `exponential-push-relay-transport-probe`

// ── Stats (EXP-553) ───────────────────────────────────────────────────────────

export interface RelayStats extends CompatSendStats {
  /** EXP-1289: deep health checks run, and how many of them failed. */
  deepChecks: number
  deepCheckFailures: number
  lastDeepCheckAt: number | null
  lastDeepCheckOk: boolean | null
}

export function createRelayApp(options: RelayAppOptions) {
  const {
    secret,
    trustProxy,
    getMessaging,
    fcmDeadlineMs = FCM_DEADLINE_MS,
    now = Date.now,
    log = consoleLogger,
  } = options

  function secretMatches(provided: string | null): boolean {
    if (!provided || secret === null) return false
    const a = Buffer.from(provided)
    const b = Buffer.from(secret)
    return a.length === b.length && timingSafeEqual(a, b)
  }

  // Failed-auth attempts on ANY route (operator secret, install secret,
  // capability) draw from this one per-IP bucket.
  const failedAuth = createIpLimiter(RATE_LIMIT_MAX, RATE_LIMIT_WINDOW_MS, now)
  const rateLimitHit = failedAuth.hit

  function clientIp(headers: Headers, fallback = `unknown`): string {
    return clientIpFrom(headers, trustProxy, fallback)
  }

  /** 401 (or 429 past the failed-attempt throttle) for a request without the operator secret, else null. */
  function rejectUnauthorized(c: Context): Response | null {
    // No operator secret: the secret-gated routes simply do not exist.
    if (secret === null) return c.json({ error: `Not found` }, 404)
    if (secretMatches(c.req.raw.headers.get(`x-relay-secret`))) return null
    // Only failed-auth attempts are rate limited (a brute-force throttle).
    // Secret-bearing traffic is trusted and never throttled: the web app fans
    // out one POST per push recipient from a single egress IP, so any per-IP
    // budget would silently drop legitimate pushes on busy issues.
    if (!rateLimitHit(clientIp(c.req.raw.headers))) {
      return c.json({ error: `Rate limit exceeded` }, 429)
    }
    return c.json({ error: `Unauthorized` }, 401)
  }

  // Monotonic since process start; exposed only on the secret-gated /stats so
  // the public /healthz stays a constant. Feeds the web app's admin
  // performance page.
  const startedAt = now()
  const stats: RelayStats = {
    sendRequests: 0,
    sendOk: 0,
    sendFailed: 0,
    sendRetried: 0,
    deadlineTimeouts: 0,
    tokensRequested: 0,
    tokensOk: 0,
    tokensFailed: 0,
    invalidTokens: 0,
    deepChecks: 0,
    deepCheckFailures: 0,
    lastDeepCheckAt: null,
    lastDeepCheckOk: null,
    lastErrorAt: null,
    lastError: null,
  }

  const app = new Hono()

  // The push API shares the failed-auth bucket, the clock and the FCM
  // client; it brings its own store and APNs client.
  const api = mountPushApi(app, {
    ...options.api,
    getMessaging,
    now,
    log,
    fcmDeadlineMs,
    clientIp,
    failedAuthHit: rateLimitHit,
  })

  // The deprecated /send shim exists only while the operator secret is set.
  if (secret !== null) {
    mountCompatSend(app, { rejectUnauthorized, getMessaging, fcmDeadlineMs, now, log, stats })
  }

  function providers() {
    return { fcm: getMessaging() !== null, apns: api.apnsConfigured() }
  }

  // Unauthenticated health check for Docker HEALTHCHECK / uptime monitors.
  // `?detail=1` reports the surfaces and providers (booleans only, so it can
  // stay public); `?deep=1` with the secret exercises the FCM transport
  // (EXP-1289). The plain answer stays the constant.
  app.get(`/healthz`, async (c) => {
    if (c.req.query(`detail`) === `1`) {
      return c.json({ ok: true, api: true, compatSend: secret !== null, providers: providers() })
    }
    if (c.req.query(`deep`) !== `1`) return c.json({ ok: true })
    const rejected = rejectUnauthorized(c)
    if (rejected) return rejected
    const messaging = getMessaging()
    if (!messaging) {
      return c.json({ ok: false, deep: true, error: `Firebase not configured` }, 503)
    }
    stats.deepChecks += 1
    stats.lastDeepCheckAt = now()
    const t0 = now()
    try {
      const response = await withDeadline(
        messaging.sendEachForMulticast(
          { tokens: [DEEP_CHECK_TOKEN], data: { probe: `1` } },
          true
        ),
        fcmDeadlineMs
      )
      stats.lastDeepCheckOk = true
      const code = response.responses[0]?.error?.code ?? `ok`
      return c.json({ ok: true, deep: true, code, ms: now() - t0 })
    } catch (err) {
      stats.deepCheckFailures += 1
      stats.lastDeepCheckOk = false
      const error = err instanceof DeadlineError ? `FCM timeout` : String(err)
      log.error(`deep health check failed after ${now() - t0}ms: ${error}`)
      return c.json({ ok: false, deep: true, error, ms: now() - t0 }, 503)
    }
  })

  // EXP-553: secret-gated stats for the web app's admin performance page.
  // The top-level counters are the /send shim's and the deep check's (the
  // page reads them); the push API's live under `api`.
  app.get(`/stats`, (c) => {
    const rejected = rejectUnauthorized(c)
    if (rejected) return rejected
    return c.json({
      ok: true,
      startedAt,
      firebaseConfigured: getMessaging() !== null,
      ...stats,
      providers: providers(),
      api: api.stats,
    })
  })

  return { app, stats, apiStats: api.stats }
}
