import { createHmac, timingSafeEqual } from "crypto"

// Magic-link token for a widget reporter's conversation (SLOP-4, the EXP-132
// scheme over the ISSUE id). The token is DETERMINISTIC —
// `<issueId>.<base64url HMAC-SHA256(BETTER_AUTH_SECRET, issueId)>` — so
// nothing secret is stored at rest (a DB leak exposes no live conversation
// links) while every outbound email still carries the SAME stable
// /support/<token> link for the issue's whole life: minting is a recompute,
// verification is a recompute + constant-time compare. Never log the token
// or persist a URL containing it anywhere (including email_deliveries
// metadata). HMAC over the issue id with BETTER_AUTH_SECRET;
// rotating BETTER_AUTH_SECRET invalidates all emailed links.

// Domain separation from the other BETTER_AUTH_SECRET HMAC uses (and from
// the pre-SLOP-4 thread tokens, which must never resolve to an issue).
const CONTEXT = `exp-reporter-issue:v1:`

function secret(): string | null {
  return process.env.BETTER_AUTH_SECRET || null
}

function computeMac(issueId: string, key: string): string {
  return createHmac(`sha256`, key)
    .update(`${CONTEXT}${issueId}`)
    .digest(`base64url`)
}

// UUID + "." + 32-byte base64url MAC (43 chars, no padding).
const TOKEN_SHAPE =
  /^([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.([A-Za-z0-9_-]{43})$/

export function mintReporterToken(issueId: string): string {
  const key = secret()
  if (!key) {
    // Better Auth itself can't run without the secret, so this only fires in
    // misconfigured test setups — fail loudly rather than mint a dud link.
    throw new Error(`BETTER_AUTH_SECRET is required to mint reporter tokens`)
  }
  return `${issueId}.${computeMac(issueId, key)}`
}

// Verify by recompute: returns the issue id the token was minted for, or
// null for anything malformed/forged — no DB work needed to reject garbage.
export function verifyReporterToken(token: string): string | null {
  const key = secret()
  if (!key) return null
  const match = TOKEN_SHAPE.exec(token)
  if (!match) return null
  const [, issueId, mac] = match
  const presented = Buffer.from(mac, `utf8`)
  const expected = Buffer.from(computeMac(issueId, key), `utf8`)
  if (presented.length !== expected.length) return null
  if (!timingSafeEqual(presented, expected)) return null
  return issueId
}
