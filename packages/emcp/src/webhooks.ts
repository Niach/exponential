// #VAPP-237 (eApps A3, docs/spec/apps.md §7.2, §12.3): an app's INBOUND
// WEBHOOKS. Pure and byte-locked by `fixtures/automations.json` `webhook`
// (its `$comment` is the spec): the signature (`t=<unix s>,v1=<hex
// HMAC-SHA256(secret, "<t>.<raw body>")>`), the verification order (size,
// header, window, signature, replay, JSON) and the secret's shape. Server
// only (node:crypto).
import { createHmac, randomBytes, timingSafeEqual } from "node:crypto"

import fixture from "../fixtures/automations.json" with { type: "json" }

export const WEBHOOK = fixture.webhook

export type WebhookError = (typeof fixture.webhook.errors)[number]

/** The `v1` HMAC (lowercase hex) of one delivery. */
export function webhookV1(secret: string, timestamp: number, body: string): string {
  return createHmac(`sha256`, secret).update(`${timestamp}.${body}`, `utf8`).digest(`hex`)
}

/** The whole header value a sender sets. */
export function webhookSignature(secret: string, timestamp: number, body: string): string {
  return `t=${timestamp},v1=${webhookV1(secret, timestamp, body)}`
}

/** A fresh secret: `secretPrefix` + base64url of `secretBytes` random bytes. */
export function newWebhookSecret(bytes: Buffer = randomBytes(fixture.webhook.secretBytes)): string {
  return `${fixture.webhook.secretPrefix}${bytes.toString(`base64url`)}`
}

/** The header's timestamp and `v1` signatures, or null when malformed. */
export function parseWebhookSignature(header: string): { timestamp: number; v1: string[] } | null {
  let timestamp: number | null = null
  const v1: string[] = []
  for (const part of header.split(`,`)) {
    const at = part.indexOf(`=`)
    if (at <= 0) return null
    const key = part.slice(0, at).trim()
    const value = part.slice(at + 1).trim()
    if (key === `t`) {
      if (!/^\d{1,12}$/.test(value) || timestamp !== null) return null
      timestamp = Number(value)
    } else if (key === `v1`) {
      if (!/^[0-9a-f]{64}$/.test(value)) return null
      v1.push(value)
    }
  }
  return timestamp === null || v1.length === 0 ? null : { timestamp, v1 }
}

const same = (a: string, b: string) => a.length === b.length && timingSafeEqual(Buffer.from(a), Buffer.from(b))

export type WebhookVerdict = { ok: true; nonce: string; body: unknown } | { ok: false; error: WebhookError }

/** One delivery's check, in order: size, header, window, signature,
 *  replay (`seen` = the hook's nonces still in the cache), JSON. `now` in
 *  unix seconds; `bytes` = the raw body's byte length. */
export function verifyWebhook(input: { header: string | null; body: string; bytes: number; secret: string; now: number; seen: readonly string[] }): WebhookVerdict {
  if (input.bytes > fixture.webhook.bodyMax) return { ok: false, error: `tooLarge` }
  if (!input.header) return { ok: false, error: `missingSignature` }
  const parsed = parseWebhookSignature(input.header)
  if (!parsed) return { ok: false, error: `badSignature` }
  if (Math.abs(input.now - parsed.timestamp) > fixture.webhook.toleranceSeconds) return { ok: false, error: `stale` }
  const expected = webhookV1(input.secret, parsed.timestamp, input.body)
  const nonce = parsed.v1.find((sig) => same(sig, expected))
  if (!nonce) return { ok: false, error: `badSignature` }
  if (input.seen.includes(nonce)) return { ok: false, error: `replayed` }
  try {
    return { ok: true, nonce, body: JSON.parse(input.body) }
  } catch {
    return { ok: false, error: `notJson` }
  }
}

/** The HTTP status of a refusal. */
export function webhookStatus(error: WebhookError | `unknownHook` | `rateLimited` | `accepted`): number {
  return (fixture.webhook.httpStatus as Record<string, number>)[error] ?? 400
}

/** The hook's URL path. */
export function webhookPath(appId: string, name: string): string {
  return fixture.webhook.path.replace(`{appId}`, appId).replace(`{name}`, name)
}

/** The run's input of a verified delivery. */
export function webhookEvent(name: string, body: unknown): { hook: string; body: unknown } {
  return { hook: name, body }
}
