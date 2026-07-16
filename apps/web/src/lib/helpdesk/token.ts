import { createHmac, timingSafeEqual } from "crypto"

// Magic-link token for helpdesk conversations (EXP-132). The token is
// DETERMINISTIC — `<threadId>.<base64url HMAC-SHA256(BETTER_AUTH_SECRET,
// threadId)>` — so nothing secret is stored at rest (a DB leak exposes no
// live conversation links) while every outbound email still carries the SAME
// stable /support/<token> link for the conversation's whole life: minting is
// a recompute, verification is a recompute + constant-time compare, and
// revocation lives on support_threads.token_revoked_at. Never log the token
// or persist a URL containing it anywhere (including email_deliveries
// metadata). Same HMAC scheme as lib/integrations/github-setup-state.ts;
// rotating BETTER_AUTH_SECRET invalidates all emailed links.

// Domain separation from the other BETTER_AUTH_SECRET HMAC uses.
const CONTEXT = `exp-support-thread:v1:`

function secret(): string | null {
  return process.env.BETTER_AUTH_SECRET || null
}

function computeMac(threadId: string, key: string): string {
  return createHmac(`sha256`, key)
    .update(`${CONTEXT}${threadId}`)
    .digest(`base64url`)
}

// UUID + "." + 32-byte base64url MAC (43 chars, no padding).
const TOKEN_SHAPE =
  /^([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.([A-Za-z0-9_-]{43})$/

export function mintSupportToken(threadId: string): string {
  const key = secret()
  if (!key) {
    // Better Auth itself can't run without the secret, so this only fires in
    // misconfigured test setups — fail loudly rather than mint a dud link.
    throw new Error(`BETTER_AUTH_SECRET is required to mint support tokens`)
  }
  return `${threadId}.${computeMac(threadId, key)}`
}

// Verify by recompute: returns the thread id the token was minted for, or
// null for anything malformed/forged — no DB work needed to reject garbage.
export function verifySupportToken(token: string): string | null {
  const key = secret()
  if (!key) return null
  const match = TOKEN_SHAPE.exec(token)
  if (!match) return null
  const [, threadId, mac] = match
  const presented = Buffer.from(mac, `utf8`)
  const expected = Buffer.from(computeMac(threadId, key), `utf8`)
  if (presented.length !== expected.length) return null
  if (!timingSafeEqual(presented, expected)) return null
  return threadId
}
