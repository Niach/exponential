import { timingSafeEqual } from "node:crypto"
import { Hono, type Context } from "hono"
import type { BatchResponse, MulticastMessage } from "firebase-admin/messaging"
import { z } from "zod"
import { mountAccountless, type AccountlessOptions } from "./accountless"
import { consoleLogger, type Logger } from "./log"
import {
  DEAD_CODES,
  DeadlineError,
  clientIpFrom,
  createIpLimiter,
  readBodyCapped,
  withDeadline,
  type RelayMessaging,
} from "./shared"

export { DeadlineError, type RelayMessaging } from "./shared"

// The relay's HTTP surface, built by `createRelayApp` so the tests can drive
// it with a fake messaging client and a fake clock (EXP-1289). `index.ts`
// wires the real environment: the secret, firebase-admin, the port.

export interface RelayAppOptions {
  /**
   * `PUSH_RELAY_SECRET`: shared with the web app. Null = accountless-only
   * (EXP-1428): every secret-gated route (/send, /stats, /healthz?deep=1)
   * answers 404, as if the v1 mode did not exist.
   */
  secret: string | null
  /** `TRUST_PROXY=true`: honor the reverse proxy's forwarded client address. */
  trustProxy: boolean
  /** The firebase-admin messaging client, or null while Firebase is not configured. */
  getMessaging: () => RelayMessaging | null
  /** Budget for one /send, deadline included (see FCM_DEADLINE_MS). */
  fcmDeadlineMs?: number
  now?: () => number
  log?: Logger
  /** EXP-1428: the accountless mode (/v2/*); mounted only when given. */
  accountless?: Omit<AccountlessOptions, `getMessaging` | `now` | `log` | `clientIp` | `failedAuthHit` | `fcmDeadlineMs`>
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

// ── FCM deadline ──────────────────────────────────────────────────────────────

// firebase-admin exposes no per-call timeout/abort, so slow FCM would hold
// every /send response — and the web app's fetch-pool slot behind it — open
// indefinitely. The web app aborts its POST after 10s (REV2-3); answering 504
// just under that keeps the failure visible to the caller instead of an abort.
// The orphaned multicast keeps settling in the background (its sends may still
// deliver); only that batch's invalid-token pruning is lost, which the next
// successful send or the web app's stale-token sweep recovers.
export const FCM_DEADLINE_MS = 8_000

// ── Transport retry (EXP-1289) ────────────────────────────────────────────────

// firebase-admin opens ONE HTTP/2 session per sendEach call and closes it
// after. Over seven weeks of prod traffic about one multicast in seventy died
// on that session's TLS handshake ("Session error while making requests:
// UNABLE_TO_VERIFY_LEAF_SIGNATURE", code messaging/app/network-error) while
// the next call, seconds later, went through — a transient of the handshake,
// not of the credential or the CA store. firebase-admin surfaces every such
// session failure under this one code, so a second attempt is a fresh session
// and a fresh handshake; nothing else about the request changes.
//
// ONE retry, and only while the first attempt was quick: the web app's fetch
// aborts at 10s, so a retry that starts past half the deadline would most
// likely just turn a fast failure into a slow one. The retry runs under the
// budget the first attempt left.
export const TRANSPORT_ERROR_CODE = `messaging/app/network-error`

export function isTransportError(err: unknown): boolean {
  if (typeof err !== `object` || err === null) return false
  const code = (err as { code?: unknown }).code
  if (code === TRANSPORT_ERROR_CODE) return true
  const message = (err as { message?: unknown }).message
  return typeof message === `string` && message.startsWith(`Session error while making requests`)
}

// ── Deep health check (EXP-1289) ──────────────────────────────────────────────

// `/healthz?deep=1` (secret-gated) sends a DRY-RUN multicast to a token that
// cannot exist. FCM validates the request and answers with a per-token
// result — the transport (auth token, HTTP/2 session, TLS) is exercised end to
// end and no device is touched. Any settled per-token result means the
// transport works; only a thrown session error or the deadline means it does
// not. The plain `/healthz` stays the constant Docker's HEALTHCHECK polls.
export const DEEP_CHECK_TOKEN = `exponential-push-relay-transport-probe`

// ── Stats (EXP-553) ───────────────────────────────────────────────────────────

export interface RelayStats {
  sendRequests: number
  sendOk: number
  sendFailed: number
  /** EXP-1289: multicasts that needed the one transport retry (counted whether or not it then succeeded). */
  sendRetried: number
  deadlineTimeouts: number
  tokensRequested: number
  tokensOk: number
  tokensFailed: number
  invalidTokens: number
  /** EXP-1289: deep health checks run, and how many of them failed. */
  deepChecks: number
  deepCheckFailures: number
  lastDeepCheckAt: number | null
  lastDeepCheckOk: boolean | null
  lastErrorAt: number | null
  lastError: string | null
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

  // Failed-auth attempts on EITHER mode draw from this one per-IP bucket.
  const failedAuth = createIpLimiter(RATE_LIMIT_MAX, RATE_LIMIT_WINDOW_MS, now)
  const rateLimitHit = failedAuth.hit

  function clientIp(headers: Headers, fallback = `unknown`): string {
    return clientIpFrom(headers, trustProxy, fallback)
  }

  /** 401 (or 429 past the failed-attempt throttle) for a request without the secret, else null. */
  function rejectUnauthorized(c: Context): Response | null {
    // Accountless-only relay: the v1 surface is simply absent.
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

  function noteSendError(message: string): void {
    stats.sendFailed += 1
    stats.lastError = message
    stats.lastErrorAt = now()
  }

  const app = new Hono()

  // EXP-1428: the accountless mode shares the failed-auth bucket, the clock
  // and the FCM client; it brings its own store and APNs client.
  const accountless = options.accountless
    ? mountAccountless(app, {
        ...options.accountless,
        getMessaging,
        now,
        log,
        fcmDeadlineMs,
        clientIp,
        failedAuthHit: rateLimitHit,
      })
    : null

  // Unauthenticated health check for Docker HEALTHCHECK / uptime monitors.
  // `?deep=1` with the secret exercises the FCM transport (EXP-1289).
  // `?modes=1` (EXP-1428) reports which modes and providers are configured:
  // booleans only, so it can stay public; the plain answer stays the constant.
  app.get(`/healthz`, async (c) => {
    if (c.req.query(`modes`) === `1`) {
      return c.json({
        ok: true,
        modes: { v1: secret !== null, accountless: accountless !== null },
        providers: {
          fcm: getMessaging() !== null,
          apns: accountless?.apnsConfigured() ?? false,
        },
      })
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
  // Same auth + failed-attempt throttle stance as /send.
  app.get(`/stats`, (c) => {
    const rejected = rejectUnauthorized(c)
    if (rejected) return rejected
    return c.json({
      ok: true,
      startedAt,
      firebaseConfigured: getMessaging() !== null,
      ...stats,
      ...(accountless ? { accountless: accountless.stats } : {}),
    })
  })

  app.post(`/send`, async (c) => {
    const rejected = rejectUnauthorized(c)
    if (rejected) return rejected

    // Reject oversized bodies before parsing JSON — the relay only ever needs
    // tokens + a short notification. Cheap pre-check on the declared length…
    const contentLength = Number.parseInt(
      c.req.raw.headers.get(`content-length`) ?? `0`,
      10
    )
    if (contentLength > MAX_BODY_BYTES) {
      return c.json({ error: `Body too large` }, 413)
    }

    const messaging = getMessaging()
    if (!messaging) {
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
    stats.sendRequests += 1
    stats.tokensRequested += tokens.length

    // Android is DATA-ONLY (EXP-264). A top-level `notification` block makes the
    // Android FCM SDK render the tray entry itself and SKIP onMessageReceived
    // whenever the app is backgrounded or killed — exactly the cases where the
    // app most needs the wake-up to sync and prefetch the issue behind the
    // notification, so taps landed on blank screens. Without it every message is
    // delivered to the service, which builds the same tray entry AND kicks sync.
    //
    // The title/body copy into `data` happens HERE rather than in the web app so
    // it is atomic with going data-only: no deploy-order window where a relay
    // dropped the notification block while the caller still only sent it there.
    // Android builds predating this already read data["title"]/data["body"]
    // (FcmService falls back to them), so no CLIENT_MIN_VERSION bump is needed.
    //
    // iOS is unaffected: the explicit apns block below is the whole wire payload
    // for APNs, so visible alerts stay byte-identical.
    //
    // Deliberately NOT set: `collapseKey` (it would drop distinct
    // per-notification payloads — the clients already coalesce per issue) and a
    // custom `ttl` (the FCM default is right for user-facing notifications).
    const dataWithDisplay = {
      title: notification.title,
      ...(notification.body !== undefined ? { body: notification.body } : {}),
      // Caller-supplied keys win: an explicit data.title from the web app is
      // more specific than the display copy above.
      ...data,
    }

    const message: MulticastMessage = {
      tokens,
      data: dataWithDisplay,
      android: {
        priority: `high`,
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
    }

    const t0 = now()
    let response: BatchResponse | undefined
    let retried = false
    while (response === undefined) {
      const elapsed = now() - t0
      try {
        response = await withDeadline(
          messaging.sendEachForMulticast(message),
          Math.max(1, fcmDeadlineMs - elapsed)
        )
      } catch (err) {
        if (err instanceof DeadlineError) {
          stats.deadlineTimeouts += 1
          noteSendError(`FCM deadline exceeded`)
          log.error(
            `FCM multicast exceeded ${fcmDeadlineMs}ms deadline (${tokens.length} tokens${retried ? `, after a transport retry` : ``})`
          )
          return c.json({ error: `FCM timeout` }, 504)
        }
        const failedAt = now() - t0
        if (!retried && isTransportError(err) && failedAt < fcmDeadlineMs / 2) {
          retried = true
          stats.sendRetried += 1
          log.warn(
            `FCM multicast transport error after ${failedAt}ms, retrying once on a fresh session (${tokens.length} tokens): ${String(err)}`
          )
          continue
        }
        noteSendError(String(err))
        log.error(
          `FCM multicast failed${retried ? ` (after a transport retry)` : ``}:`,
          err
        )
        return c.json({ error: `FCM error` }, 500)
      }
    }
    stats.sendOk += 1
    stats.tokensOk += response.successCount
    stats.tokensFailed += response.failureCount

    const invalidTokens: string[] = []
    response.responses.forEach((res, i) => {
      if (res.success) return
      const code = res.error?.code
      if (code && DEAD_CODES.has(code)) {
        invalidTokens.push(tokens[i])
      } else {
        log.error(
          `send error token=${tokens[i]?.slice(0, 12)}… code=${res.error?.code ?? `unknown`}`
        )
      }
    })

    stats.invalidTokens += invalidTokens.length
    return c.json({ invalidTokens })
  })

  return { app, stats, accountlessStats: accountless?.stats ?? null }
}
