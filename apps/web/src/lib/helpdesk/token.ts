import { createHash, randomBytes, timingSafeEqual } from "crypto"

// Magic-link token for helpdesk conversations. The raw token IS the
// credential: it exists only inside emailed /support/<token> URLs. The DB
// stores sha256(token) — never log the raw token or persist a URL containing
// it anywhere (including email_deliveries metadata).

// 32 random bytes, base64url → 43 chars, URL-safe with no padding.
export function generateSupportToken(): string {
  return randomBytes(32).toString(`base64url`)
}

export function hashSupportToken(token: string): string {
  return createHash(`sha256`).update(token).digest(`hex`)
}

// Cheap shape reject before any hashing/DB work: exactly the base64url
// alphabet at the length generateSupportToken emits.
const TOKEN_SHAPE = /^[A-Za-z0-9_-]{43}$/

export function isValidSupportTokenShape(token: string): boolean {
  return TOKEN_SHAPE.test(token)
}

// Constant-time hash comparison. The row is already fetched by hash equality,
// so this is defense in depth against index-lookup timing, not the primary
// check.
export function supportTokenHashMatches(
  token: string,
  storedHash: string
): boolean {
  const computed = Buffer.from(hashSupportToken(token), `hex`)
  const stored = Buffer.from(storedHash, `hex`)
  if (computed.length !== stored.length) return false
  return timingSafeEqual(computed, stored)
}
