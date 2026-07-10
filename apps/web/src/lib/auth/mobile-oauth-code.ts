import { createHash, randomBytes, timingSafeEqual } from "crypto"

// One-time PKCE-style codes for the mobile OAuth handoff (REV-13).
//
// The `exponential://oauth-return` deep link used to carry the RAW Better Auth
// session token — a full bearer credential — through the OS's custom-scheme
// dispatch, where any co-installed app registering the same scheme can win the
// implicit-intent resolution (Android custom schemes can never be App Links;
// Linux/Windows scheme handlers have the same interception class). Now the
// link carries only a short-TTL, single-use code bound to an S256 challenge
// the native app presented at /api/mobile-oauth-start; the token only ever
// travels over the TLS response of POST /api/mobile-oauth-exchange, which
// requires the app-held code_verifier. An intercepted code is useless.
//
// The store is in-process memory: fine for the single-container Coolify
// deploys and dev (globalThis survives HMR so the code minted by the return
// page survives to the exchange), but a multi-replica web deployment would
// need a DB/redis-backed store — a code minted on one replica could land its
// exchange on another. A server restart between return and exchange voids the
// code; the user just retries sign-in.

export const MOBILE_OAUTH_CODE_TTL_MS = 5 * 60 * 1000

// RFC 7636 §4.2: code_challenge is base64url-no-pad (43–128 chars).
export function isValidCodeChallenge(value: string): boolean {
  return /^[A-Za-z0-9_-]{43,128}$/.test(value)
}

// RFC 7636 §4.1: code_verifier is unreserved chars (43–128 chars).
export function isValidCodeVerifier(value: string): boolean {
  return /^[A-Za-z0-9\-._~]{43,128}$/.test(value)
}

type Entry = {
  token: string
  codeChallenge: string
  expiresAt: number
}

// globalThis-anchored so dev HMR doesn't drop codes between return and
// exchange; prod is a single Bun process.
const store = ((globalThis as Record<string, unknown>).__expMobileOauthCodes ??=
  new Map<string, Entry>()) as Map<string, Entry>

function sweepExpired(now: number): void {
  for (const [code, entry] of store) {
    if (entry.expiresAt <= now) store.delete(code)
  }
}

function s256(codeVerifier: string): string {
  return createHash(`sha256`).update(codeVerifier).digest(`base64url`)
}

// Mint a single-use code that redeems for `token` when presented together
// with the verifier matching `codeChallenge`.
export function mintMobileOauthCode(
  token: string,
  codeChallenge: string,
  now = Date.now()
): string {
  sweepExpired(now)
  const code = randomBytes(32).toString(`base64url`)
  store.set(code, { token, codeChallenge, expiresAt: now + MOBILE_OAUTH_CODE_TTL_MS })
  return code
}

// Redeem a code. The code is consumed on ANY lookup hit — even a failed
// verifier burns it (single-use, no oracle). Returns the session token, or
// null when the code is unknown/expired or the verifier doesn't hash to the
// bound challenge.
export function exchangeMobileOauthCode(
  code: string,
  codeVerifier: string,
  now = Date.now()
): string | null {
  const entry = store.get(code)
  if (!entry) return null
  store.delete(code)
  if (entry.expiresAt <= now) return null
  if (!isValidCodeVerifier(codeVerifier)) return null
  const expected = Buffer.from(entry.codeChallenge)
  const actual = Buffer.from(s256(codeVerifier))
  if (expected.length !== actual.length) return null
  if (!timingSafeEqual(expected, actual)) return null
  return entry.token
}
