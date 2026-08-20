import { inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { fcmTokens } from "@/db/schema"
import { recordPushBatch } from "@/lib/metrics/registry"

// ── Relay config (lazy singleton) ─────────────────────────────────────────────

let relayUrl: string | null | undefined

function getRelayUrl(): string | null {
  if (relayUrl !== undefined) return relayUrl

  const url = process.env.PUSH_RELAY_URL
  if (!url) {
    console.warn(`[fcm] PUSH_RELAY_URL not set — push notifications disabled`)
    relayUrl = null
    return relayUrl
  }
  relayUrl = url
  return relayUrl
}

// ── Types ─────────────────────────────────────────────────────────────────────

export type FcmPayload = {
  title: string
  body?: string
  data: Record<string, string>
}

// One push recipient. Its optional `data` is merged OVER the shared payload
// data for this user only — it carries the keys that differ per recipient
// while everything else stays shared (EXP-264: the recipient's own
// notificationId, so a tapped push can mark exactly that inbox row read).
export type FcmRecipient = {
  userId: string
  data?: Record<string, string>
}

// ── Main export ───────────────────────────────────────────────────────────────

// The relay (and FCM multicast) accept at most 500 tokens per request.
const MAX_TOKENS_PER_REQUEST = 500

// Bun's outbound fetch shares ONE process-global connection pool (256 slots)
// with the Electric shape proxy, so relay POSTs must be bounded in both time
// and count: a wedged relay holding sockets open would otherwise starve
// long-poll proxying and stall real-time sync for every client (REV2-3).
const RELAY_TIMEOUT_MS = 10_000
const RELAY_CONCURRENCY = 8

/**
 * Push one payload to every listed recipient's devices. Tokens are fetched in
 * ONE query and grouped per user; the relay POSTs stay per-user (each
 * recipient's `data` differs — at minimum the userId, so multi-account clients
 * can route the tap), but run through a small worker pool with a per-request
 * timeout so a slow or wedged relay can never pin more than RELAY_CONCURRENCY
 * fetch-pool slots. Individual request failures are logged and never throw.
 */
export async function sendToUsers(
  recipients: FcmRecipient[],
  payload: FcmPayload
): Promise<void> {
  const url = getRelayUrl()
  if (!url) return
  if (recipients.length === 0) return

  const dataByUser = new Map<string, Record<string, string> | undefined>()
  for (const recipient of recipients) {
    dataByUser.set(recipient.userId, recipient.data)
  }

  const rows = await db
    .select({ userId: fcmTokens.userId, token: fcmTokens.token })
    .from(fcmTokens)
    .where(inArray(fcmTokens.userId, [...dataByUser.keys()]))
  if (rows.length === 0) return

  const tokensByUser = new Map<string, string[]>()
  for (const row of rows) {
    const list = tokensByUser.get(row.userId)
    if (list) list.push(row.token)
    else tokensByUser.set(row.userId, [row.token])
  }

  const headers: Record<string, string> = {
    "Content-Type": `application/json`,
  }
  const relaySecret = process.env.PUSH_RELAY_SECRET
  if (relaySecret) {
    headers[`x-relay-secret`] = relaySecret
  }

  const requests: Array<{ userId: string; tokens: string[] }> = []
  for (const [userId, tokens] of tokensByUser) {
    for (let i = 0; i < tokens.length; i += MAX_TOKENS_PER_REQUEST) {
      requests.push({ userId, tokens: tokens.slice(i, i + MAX_TOKENS_PER_REQUEST) })
    }
  }

  const invalidTokens: string[] = []
  let next = 0
  let requestsOk = 0
  let requestsFailed = 0
  let lastError: string | undefined

  async function worker(): Promise<void> {
    while (next < requests.length) {
      const req = requests[next]
      next += 1
      try {
        const res = await fetch(`${url}/send`, {
          method: `POST`,
          headers,
          signal: AbortSignal.timeout(RELAY_TIMEOUT_MS),
          body: JSON.stringify({
            tokens: req.tokens,
            notification: { title: payload.title, body: payload.body },
            // Shared payload data first, then this recipient's own keys. The
            // user id is written LAST so per-recipient data can never clobber
            // it: it rides along so multi-account clients can route a tapped
            // notification into the signed-in account it belongs to instead of
            // whichever account happens to be active.
            data: {
              ...payload.data,
              ...dataByUser.get(req.userId),
              userId: req.userId,
            },
          }),
        })

        if (!res.ok) {
          requestsFailed += 1
          lastError = `relay responded ${res.status}`
          console.error(`[fcm] relay responded ${res.status}:`, await res.text())
          continue
        }

        const json = (await res.json()) as { invalidTokens?: string[] }
        invalidTokens.push(...(json.invalidTokens ?? []))
        requestsOk += 1
      } catch (err) {
        requestsFailed += 1
        lastError = String(err)
        console.error(`[fcm] relay request failed:`, err)
      }
    }
  }

  await Promise.all(
    Array.from({ length: Math.min(RELAY_CONCURRENCY, requests.length) }, () =>
      worker()
    )
  )

  recordPushBatch({
    requestsOk,
    requestsFailed,
    invalidTokens: invalidTokens.length,
    lastError,
  })

  if (invalidTokens.length > 0) {
    // FCM invalidates a token for the whole device, so delete every account's
    // registration of it — not just the notified user's row.
    await db.delete(fcmTokens).where(inArray(fcmTokens.token, invalidTokens))
  }
}
