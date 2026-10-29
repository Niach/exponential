import type { BatchResponse, MulticastMessage } from "firebase-admin/messaging"

// Pieces both relay modes share: the FCM client slice, the dead-token codes,
// the deadline wrapper, the capped body read and the per-IP buckets. They
// lived in `app.ts` until the accountless mode (EXP-1428) needed them too;
// `app.ts` re-exports what it always exported.

/** The slice of firebase-admin Messaging the relay uses. */
export interface RelayMessaging {
  sendEachForMulticast(message: MulticastMessage, dryRun?: boolean): Promise<BatchResponse>
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

export const DEAD_CODES = new Set([
  `messaging/registration-token-not-registered`,
  `messaging/mismatched-credential`,
])

// ── Deadline ──────────────────────────────────────────────────────────────────

export class DeadlineError extends Error {}

export function withDeadline<T>(promise: Promise<T>, ms: number): Promise<T> {
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

// ── Capped body read ──────────────────────────────────────────────────────────

// Read a request body with a hard byte cap, aborting the stream as soon as it
// exceeds the cap. Returns null when over the cap; a read error yields an
// empty string (which then fails JSON parsing as before).
export async function readBodyCapped(
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

// ── Per-IP buckets ────────────────────────────────────────────────────────────

// Forwarded headers are client-forgeable: every spoofed value would mint its
// own fresh rate-limit bucket. They are honored only when TRUST_PROXY says a
// reverse proxy we control fronts the relay — and then only the RIGHTMOST
// x-forwarded-for entry (the one that proxy appended) counts. Otherwise all
// requests share the fallback bucket.
//
// Loose IPv4/IPv6 shape check — anything else falls back to the shared bucket.
const IP_RE = /^(?:\d{1,3}(?:\.\d{1,3}){3}|[0-9a-fA-F:]+)$/

export function clientIpFrom(
  headers: Headers,
  trustProxy: boolean,
  fallback = `unknown`
): string {
  if (!trustProxy) return fallback
  const forwarded = headers
    .get(`x-forwarded-for`)
    ?.split(`,`)
    .map((entry) => entry.trim())
    .filter(Boolean)
  const candidate = forwarded?.at(-1) ?? headers.get(`x-real-ip`)?.trim()
  if (candidate && IP_RE.test(candidate)) return candidate
  return fallback
}

/** A fixed-window counter per key: `hit` answers whether the key is still within `max`. */
export function createIpLimiter(max: number, windowMs: number, now: () => number) {
  interface RateBucket {
    count: number
    resetAt: number
  }

  const buckets = new Map<string, RateBucket>()

  function hit(key: string): boolean {
    const current = now()
    const bucket = buckets.get(key)
    if (!bucket || bucket.resetAt <= current) {
      buckets.set(key, { count: 1, resetAt: current + windowMs })
      return true
    }
    bucket.count += 1
    return bucket.count <= max
  }

  // Best-effort sweep of the map so it can't grow unbounded under traffic.
  setInterval(() => {
    const current = now()
    for (const [key, bucket] of buckets) {
      if (bucket.resetAt <= current) buckets.delete(key)
    }
  }, windowMs).unref?.()

  return { hit }
}
