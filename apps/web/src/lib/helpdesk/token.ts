import { randomBytes, timingSafeEqual } from "crypto"

// Magic-link token for helpdesk conversations. The raw token IS the
// credential and is stored on the thread row so every outbound email can
// carry the SAME stable /support/<token> link for the conversation's whole
// life (deliberate trade-off vs hash-only storage: a DB leak exposes live
// links, but reporters never lose access when a reply lands). Never log the
// token or persist a URL containing it anywhere else (including
// email_deliveries metadata).

// 32 random bytes, base64url → 43 chars, URL-safe with no padding.
export function generateSupportToken(): string {
  return randomBytes(32).toString(`base64url`)
}

// Cheap shape reject before any DB work: exactly the base64url alphabet at
// the length generateSupportToken emits.
const TOKEN_SHAPE = /^[A-Za-z0-9_-]{43}$/

export function isValidSupportTokenShape(token: string): boolean {
  return TOKEN_SHAPE.test(token)
}

// Constant-time comparison. The row is already fetched by token equality, so
// this is defense in depth against index-lookup timing, not the primary
// check.
export function supportTokensMatch(token: string, stored: string): boolean {
  const presented = Buffer.from(token, `utf8`)
  const expected = Buffer.from(stored, `utf8`)
  if (presented.length !== expected.length) return false
  return timingSafeEqual(presented, expected)
}
