// Steer relay tickets — the ONLY credential the relay ever sees.
//
// The web app mints a short-lived ticket (steer.mintTicket, after checking the
// caller's workspace permission); the relay verifies signature + expiry and
// trusts the claims. Compact HS256 format (NOT a full JWT — no header, no alg
// negotiation): `base64url(JSON claims) + "." + base64url(HMAC-SHA256)`.
// Shared by apps/web (sign) and apps/steer-relay (verify) so the format can't
// drift.

import { createHmac, timingSafeEqual } from "node:crypto"

// `public_viewer` (the anonymous public-activity audience) was removed in
// EXP-90 — the relay rejects tickets carrying any unknown role, so stale
// instances that still mint it get a closed socket, never data.
export type SteerRole = `control` | `publisher` | `viewer`
export type SteerPerm = `view` | `steer`

export interface SteerTicketClaims {
  /** userId of the authenticated caller. */
  sub: string
  /** teamId the ticket is scoped to (empty string for control tickets). */
  team: string
  /** Display name, shown in viewer presence. */
  name?: string
  /** Human device label (control tickets). */
  deviceLabel?: string
  /** coding_sessions.id (publisher/viewer tickets). */
  sessionId?: string
  role: SteerRole
  perm: SteerPerm
  /** Unix seconds. */
  iat: number
  /** Unix seconds — connect window; the socket outlives it once established. */
  exp: number
}

function b64url(buf: Buffer): string {
  return buf.toString(`base64url`)
}

function hmac(payloadB64: string, secret: string): Buffer {
  return createHmac(`sha256`, secret).update(payloadB64).digest()
}

export function signSteerTicket(
  claims: SteerTicketClaims,
  secret: string
): string {
  const payload = b64url(Buffer.from(JSON.stringify(claims), `utf8`))
  return `${payload}.${b64url(hmac(payload, secret))}`
}

export type VerifyResult =
  | { ok: true; claims: SteerTicketClaims }
  | { ok: false; reason: `malformed` | `bad_signature` | `expired` }

export function verifySteerTicket(
  token: string,
  secret: string,
  nowSeconds = Math.floor(Date.now() / 1000)
): VerifyResult {
  const dot = token.indexOf(`.`)
  if (dot <= 0 || dot === token.length - 1) return { ok: false, reason: `malformed` }
  const payload = token.slice(0, dot)
  const sig = token.slice(dot + 1)

  let expected: Buffer
  let provided: Buffer
  try {
    expected = hmac(payload, secret)
    provided = Buffer.from(sig, `base64url`)
  } catch {
    return { ok: false, reason: `malformed` }
  }
  if (
    provided.length !== expected.length ||
    !timingSafeEqual(provided, expected)
  ) {
    return { ok: false, reason: `bad_signature` }
  }

  let claims: SteerTicketClaims
  try {
    claims = JSON.parse(Buffer.from(payload, `base64url`).toString(`utf8`))
  } catch {
    return { ok: false, reason: `malformed` }
  }
  if (
    typeof claims !== `object` ||
    claims === null ||
    typeof claims.sub !== `string` ||
    typeof claims.exp !== `number` ||
    typeof claims.role !== `string`
  ) {
    return { ok: false, reason: `malformed` }
  }
  if (claims.exp < nowSeconds) return { ok: false, reason: `expired` }
  return { ok: true, claims }
}
