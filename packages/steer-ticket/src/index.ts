// Steer relay tickets — the ONLY credential the relay ever sees.
//
// The web app mints a short-lived ticket (`steer.mintTicket` for a device's
// control socket, `voice.*` for a call); the relay verifies signature +
// expiry and trusts the claims. Compact HS256 format (NOT a full JWT — no header, no alg
// negotiation): `base64url(JSON claims) + "." + base64url(HMAC-SHA256)`.
// Shared by apps/web (sign) and apps/steer-relay (verify) so the format can't
// drift.

import { createHmac, timingSafeEqual } from "node:crypto"

// `public_viewer` (the anonymous public-activity audience) was removed in
// EXP-90 — the relay rejects tickets carrying any unknown role, so stale
// instances that still mint it get a closed socket, never data.
//
// NEW-55: `caller` (a phone/web client streaming a voice call's audio) and
// `call_host` (the voice device answering it) open CALL rooms keyed by the
// `callId` claim — binary Opus frames between exactly two sockets.
// VAPP-43: `publisher` and `viewer` are gone with the relay's session rooms
// (session content travels over eNet); the relay 401s them like any unknown
// role.
export type SteerRole = `control` | `caller` | `call_host`

// EXP-312 / VAPP-43: the old `perm`, `name`, `sessionId` and `live` claims
// are GONE with the session tickets.
// EXP-710: the control ticket's `deviceLabel` claim is GONE too — presence is
// the deviceId → socket map and nothing more since EXP-672, so no reader was
// left. Verification stays TOLERANT of it (and of any other legacy claim): the
// claims object is parsed, never schema-checked, so a ticket minted seconds
// before a deploy still verifies and connects.
export interface SteerTicketClaims {
  /** userId of the authenticated caller. */
  sub: string
  /** teamId the ticket is scoped to (empty string for control tickets). */
  team: string
  /** NEW-55: on a CALLER ticket, the steer deviceId of the voice device to
   * ring; on a CALL_HOST ticket, the device answering. */
  deviceId?: string
  /** NEW-55: on a CALLER ticket, the account the `deviceId` above is
   * registered under (presence is indexed by device OWNER). Absent = `sub`. */
  deviceOwnerId?: string
  /** NEW-55: the call room this ticket opens (`caller`) or answers
   * (`call_host`). Minted by `voice.call` / `voice.callHostTicket`; the relay
   * admits a host only when its `sub` IS the rung device's owner. */
  callId?: string
  /** NEW-55: the agent the call talks to (caller tickets): forwarded in the
   * `call_ring` so the voice device knows whose conversation to open. */
  agentId?: string
  role: SteerRole
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
