import { and, inArray, isNotNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { fcmTokens } from "@/db/schema"
import { recordPushBatch } from "@/lib/metrics/registry"
import { PUSH_OVERHEAD, pushSeal } from "@/lib/push-seal"
import { QUESTION_PUSH_DATA_KEYS } from "@exp/domain-contract/question-push"

// ── Relay config (lazy singleton) ─────────────────────────────────────────────

let relayUrl: string | null | undefined

export function getRelayUrl(): string | null {
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
  // Sealed (/push) deliveries only: the relay maps it to apns-collapse-id /
  // FCM collapse_key so a newer push replaces an older one on the device.
  collapseId?: string
  // #EXP-1815 (notification-delivery.json): a SILENT push (the dismiss of a
  // read notification): the relay's POST /push/silent, APNs background +
  // content-available, FCM data-only at normal priority. Never shows.
  silent?: boolean
}

// One push recipient. Its optional `data` is merged OVER the shared payload
// data for this user only — it carries the keys that differ per recipient
// while everything else stays shared (EXP-264: the recipient's own
// notificationId, so a tapped push can mark exactly that inbox row read).
export type FcmRecipient = {
  userId: string
  data?: Record<string, string>
}

// ── Sealed payload sizing ─────────────────────────────────────────────────────

// The relay refuses a sealed blob over 3072 bytes (decoded) with 413.
export const MAX_SEALED_BYTES = 3072
const MAX_SEALED_PLAINTEXT = MAX_SEALED_BYTES - PUSH_OVERHEAD
// The keys a tap cannot route without: the account, the inbox row to mark
// read, and the notification kind. Everything else is dropped before these.
const ESSENTIAL_DATA_KEYS = new Set([`userId`, `notificationId`, `type`])
// #VAPP-45 (question-push.json): a question's tree-sealed text, the first
// thing dropped over budget (the phone then shows the push body).
const SEALED_TEXT_KEY = QUESTION_PUSH_DATA_KEYS.sealedText

const utf8 = new TextEncoder()

/** The longest-body JSON `{title, body, data}` that fits, or null when even
 * an empty body does not. Body is cut on code points and ends in "…". */
function fitBody(
  title: string,
  body: string | undefined,
  data: Record<string, string>
): Uint8Array | null {
  const encode = (b: string | undefined) =>
    utf8.encode(JSON.stringify({ title, body: b, data }))
  const full = encode(body)
  if (full.length <= MAX_SEALED_PLAINTEXT) return full
  const chars = [...(body ?? ``)]
  let lo = 0
  let hi = chars.length - 1
  let best: Uint8Array | null = null
  while (lo <= hi) {
    const mid = (lo + hi) >> 1
    const candidate = encode(mid === 0 ? `` : `${chars.slice(0, mid).join(``)}…`)
    if (candidate.length <= MAX_SEALED_PLAINTEXT) {
      best = candidate
      lo = mid + 1
    } else hi = mid - 1
  }
  return best
}

/**
 * The plaintext sealed for one device: JSON `{title, body, data}` with the
 * recipient's merged data. Over budget (sealed > 3072 bytes)
 * it degrades in order: (0) a question's `sealedText` is dropped (#VAPP-45),
 * (1) the body is truncated, (2) data keys other than
 * userId / notificationId / type are dropped (body re-fitted from the
 * original), (3) the title is cut too. The routing keys always survive.
 */
export function sealedPlaintext(
  title: string,
  body: string | undefined,
  data: Record<string, string>
): Uint8Array {
  if (SEALED_TEXT_KEY in data) {
    const whole = utf8.encode(JSON.stringify({ title, body, data }))
    if (whole.length <= MAX_SEALED_PLAINTEXT) return whole
    const { [SEALED_TEXT_KEY]: _dropped, ...rest } = data
    return sealedPlaintext(title, body, rest)
  }
  const fitted = fitBody(title, body, data)
  if (fitted) return fitted
  const essential = Object.fromEntries(
    Object.entries(data).filter(([k]) => ESSENTIAL_DATA_KEYS.has(k))
  )
  const slim = fitBody(title, body, essential)
  if (slim) return slim
  const titleChars = [...title]
  for (let n = Math.min(titleChars.length, MAX_SEALED_PLAINTEXT); n >= 0; n -= 64) {
    const cut = fitBody(`${titleChars.slice(0, n).join(``)}…`, undefined, essential)
    if (cut) return cut
  }
  return utf8.encode(JSON.stringify({ title: ``, data: essential }))
}

// ── Main export ───────────────────────────────────────────────────────────────

// Bun's outbound fetch shares ONE process-global connection pool (256 slots)
// with the Electric shape proxy, so relay POSTs must be bounded in both time
// and count: a wedged relay holding sockets open would otherwise starve
// long-poll proxying and stall real-time sync for every client (REV2-3).
const RELAY_TIMEOUT_MS = 10_000
const RELAY_CONCURRENCY = 8

// The accountless push API (EXP-1428): one sealed POST /push per device,
// authorized by the capability the device minted for us.
type RelayRequest = { userId: string; capability: string; pushPublicKey: string }

/**
 * Push one payload to every listed recipient's devices. Devices are fetched in
 * ONE query; only rows carrying a relay capability + push public key can be
 * reached (NEW-14 retired the bare-token /send shim; leftover bare rows are
 * skipped and age out through fcm-token-sweep). Each device gets its own
 * sealed POST /push with its recipient's merged `data` (at minimum the userId,
 * so multi-account clients can route the tap); 410/401 = the install or
 * capability is gone: the row is deleted. Every POST runs through a small
 * worker pool with a per-request timeout so a slow or wedged relay can never
 * pin more than RELAY_CONCURRENCY fetch-pool slots. Individual request
 * failures are logged and never throw.
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
    .select({
      userId: fcmTokens.userId,
      relayCapability: fcmTokens.relayCapability,
      pushPublicKey: fcmTokens.pushPublicKey,
    })
    .from(fcmTokens)
    .where(
      and(
        inArray(fcmTokens.userId, [...dataByUser.keys()]),
        isNotNull(fcmTokens.relayCapability),
        isNotNull(fcmTokens.pushPublicKey)
      )
    )

  const requests: RelayRequest[] = []
  for (const row of rows) {
    if (!row.relayCapability || !row.pushPublicKey) continue
    requests.push({
      userId: row.userId,
      capability: row.relayCapability,
      pushPublicKey: row.pushPublicKey,
    })
  }
  if (requests.length === 0) return

  // Shared payload data first, then this recipient's own keys. The user id is
  // written LAST so per-recipient data can never clobber it: it rides along so
  // multi-account clients can route a tapped notification into the signed-in
  // account it belongs to instead of whichever account happens to be active.
  const dataFor = (userId: string): Record<string, string> => ({
    ...payload.data,
    ...dataByUser.get(userId),
    userId,
  })

  const goneCapabilities: string[] = []
  let next = 0
  let requestsOk = 0
  let requestsFailed = 0
  let lastError: string | undefined

  async function sendSealed(req: RelayRequest): Promise<void> {
    const sealed = pushSeal(
      Buffer.from(req.pushPublicKey, `base64url`),
      sealedPlaintext(payload.title, payload.body, dataFor(req.userId))
    )
    const res = await fetch(payload.silent ? `${url}/push/silent` : `${url}/push`, {
      method: `POST`,
      headers: {
        "Content-Type": `application/json`,
        Authorization: `Bearer ${req.capability}`,
      },
      signal: AbortSignal.timeout(RELAY_TIMEOUT_MS),
      body: JSON.stringify({
        sealed: Buffer.from(sealed).toString(`base64url`),
        ...(payload.collapseId ? { collapseId: payload.collapseId } : {}),
      }),
    })
    if (res.ok) {
      requestsOk += 1
      return
    }
    // 410 = the install is gone, 401 = the capability was revoked or
    // expired: either way this row can never deliver again.
    if (res.status === 410 || res.status === 401) {
      goneCapabilities.push(req.capability)
      requestsOk += 1
      return
    }
    // A relay from before /push/silent answers 404: dismissing stays local
    // until it is updated; nothing to count or log per request.
    if (payload.silent && res.status === 404) {
      requestsOk += 1
      return
    }
    requestsFailed += 1
    lastError = `relay /push responded ${res.status}`
    console.error(`[fcm] relay /push responded ${res.status}:`, await res.text())
  }

  async function worker(): Promise<void> {
    while (next < requests.length) {
      const req = requests[next]
      next += 1
      try {
        await sendSealed(req)
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
    // Dead registrations: rows whose install or capability the relay
    // reported gone.
    invalidTokens: goneCapabilities.length,
    lastError,
  })

  if (goneCapabilities.length > 0) {
    // A capability belongs to exactly one install, so its value names the
    // dead row(s) on its own.
    await db
      .delete(fcmTokens)
      .where(inArray(fcmTokens.relayCapability, goneCapabilities))
  }
}
