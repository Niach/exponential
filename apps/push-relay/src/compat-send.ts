import { Hono, type Context } from "hono"
import type { BatchResponse, MulticastMessage } from "firebase-admin/messaging"
import { z } from "zod"
import type { Logger } from "./log"
import { DEAD_CODES, DeadlineError, readBodyCapped, withDeadline, type RelayMessaging } from "./shared"

// ═══ COMPAT SHIM: DEPRECATED ═══════════════════════════════════════════════════
//
// `POST /send` is NOT part of the push API (push-api.ts). It exists only for
// the Exponential iOS and Android builds already in users' hands: they hand
// RAW FCM tokens to the web server and display plaintext FCM `notification`
// messages, so they cannot open a sealed payload. Until they register installs
// themselves, the web server keeps reaching them through this route, holding
// PUSH_RELAY_SECRET (unset = this route is not mounted: 404).
//
// Behaviour is frozen: byte-identical to the pre-push-API /send (its tests in
// app.test.ts are unchanged). Do not extend it. Removal is tracked under
// NEW-14, once CLIENT_MIN_VERSION_IOS / CLIENT_MIN_VERSION_ANDROID pass the
// first builds that register installs; then this file, the secret, its
// counters on /stats and the web server's FCM-token path go together.
//
// ═══════════════════════════════════════════════════════════════════════════════

export interface CompatSendStats {
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
  lastErrorAt: number | null
  lastError: string | null
}

export interface CompatSendOptions {
  /** The secret gate shared with /stats and /healthz?deep=1: null = authorized. */
  rejectUnauthorized: (c: Context) => Response | null
  getMessaging: () => RelayMessaging | null
  fcmDeadlineMs: number
  now: () => number
  log: Logger
  /** Counted into the relay's operator stats (/stats). */
  stats: CompatSendStats
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

const MAX_BODY_BYTES = 64 * 1024

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

// ── Mount ─────────────────────────────────────────────────────────────────────

export function mountCompatSend(app: Hono, options: CompatSendOptions): void {
  const { rejectUnauthorized, getMessaging, fcmDeadlineMs, now, log, stats } = options

  function noteSendError(message: string): void {
    stats.sendFailed += 1
    stats.lastError = message
    stats.lastErrorAt = now()
  }

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
}
