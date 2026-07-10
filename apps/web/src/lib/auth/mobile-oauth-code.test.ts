import { describe, expect, it } from "vitest"
import {
  exchangeMobileOauthCode,
  isValidCodeChallenge,
  isValidCodeVerifier,
  mintMobileOauthCode,
  MOBILE_OAUTH_CODE_TTL_MS,
} from "@/lib/auth/mobile-oauth-code"

// RFC 7636 Appendix B canonical S256 vector — the same pair is asserted by
// the Android (OauthPkceTest), iOS (PkceTests), and desktop (login.rs) tests
// so all four implementations provably agree on the challenge derivation.
const RFC7636_VERIFIER = `dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk`
const RFC7636_CHALLENGE = `E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM`

describe(`mintMobileOauthCode / exchangeMobileOauthCode`, () => {
  it(`round-trips the RFC 7636 vector: mint with the challenge, exchange with the verifier`, () => {
    const code = mintMobileOauthCode(`session-token-1`, RFC7636_CHALLENGE)
    expect(exchangeMobileOauthCode(code, RFC7636_VERIFIER)).toBe(`session-token-1`)
  })

  it(`rejects a wrong verifier AND burns the code (no retry with the right one)`, () => {
    const code = mintMobileOauthCode(`session-token-2`, RFC7636_CHALLENGE)
    const wrongVerifier = `${RFC7636_VERIFIER.slice(0, -1)}X`
    expect(exchangeMobileOauthCode(code, wrongVerifier)).toBeNull()
    // Single-use even on failure: the lookup hit consumed the code.
    expect(exchangeMobileOauthCode(code, RFC7636_VERIFIER)).toBeNull()
  })

  it(`returns null for an unknown code`, () => {
    expect(exchangeMobileOauthCode(`no-such-code`, RFC7636_VERIFIER)).toBeNull()
  })

  it(`expires codes after the TTL (injected clock)`, () => {
    const t = 1_700_000_000_000
    const code = mintMobileOauthCode(`session-token-3`, RFC7636_CHALLENGE, t)
    expect(
      exchangeMobileOauthCode(code, RFC7636_VERIFIER, t + MOBILE_OAUTH_CODE_TTL_MS + 1)
    ).toBeNull()
  })

  it(`redeems just inside the TTL`, () => {
    const t = 1_700_000_000_000
    const code = mintMobileOauthCode(`session-token-4`, RFC7636_CHALLENGE, t)
    expect(
      exchangeMobileOauthCode(code, RFC7636_VERIFIER, t + MOBILE_OAUTH_CODE_TTL_MS - 1)
    ).toBe(`session-token-4`)
  })

  it(`is single-use after a successful exchange`, () => {
    const code = mintMobileOauthCode(`session-token-5`, RFC7636_CHALLENGE)
    expect(exchangeMobileOauthCode(code, RFC7636_VERIFIER)).toBe(`session-token-5`)
    expect(exchangeMobileOauthCode(code, RFC7636_VERIFIER)).toBeNull()
  })

  it(`mints distinct codes per call`, () => {
    const a = mintMobileOauthCode(`t`, RFC7636_CHALLENGE)
    const b = mintMobileOauthCode(`t`, RFC7636_CHALLENGE)
    expect(a).not.toBe(b)
  })
})

describe(`isValidCodeChallenge`, () => {
  it(`accepts base64url between 43 and 128 chars`, () => {
    expect(isValidCodeChallenge(RFC7636_CHALLENGE)).toBe(true)
    expect(isValidCodeChallenge(`a`.repeat(43))).toBe(true)
    expect(isValidCodeChallenge(`a`.repeat(128))).toBe(true)
  })

  it(`rejects out-of-range lengths and illegal chars`, () => {
    expect(isValidCodeChallenge(`a`.repeat(42))).toBe(false)
    expect(isValidCodeChallenge(`a`.repeat(129))).toBe(false)
    // `.` and `~` are verifier chars but NOT base64url.
    expect(isValidCodeChallenge(`${`a`.repeat(42)}.`)).toBe(false)
    expect(isValidCodeChallenge(`${`a`.repeat(42)}=`)).toBe(false)
    expect(isValidCodeChallenge(``)).toBe(false)
  })
})

describe(`isValidCodeVerifier`, () => {
  it(`accepts unreserved chars between 43 and 128 chars`, () => {
    expect(isValidCodeVerifier(RFC7636_VERIFIER)).toBe(true)
    expect(isValidCodeVerifier(`${`a`.repeat(39)}-._~`)).toBe(true)
    expect(isValidCodeVerifier(`a`.repeat(128))).toBe(true)
  })

  it(`rejects out-of-range lengths and illegal chars`, () => {
    expect(isValidCodeVerifier(`a`.repeat(42))).toBe(false)
    expect(isValidCodeVerifier(`a`.repeat(129))).toBe(false)
    expect(isValidCodeVerifier(`${`a`.repeat(42)}+`)).toBe(false)
    expect(isValidCodeVerifier(`${`a`.repeat(42)}/`)).toBe(false)
    expect(isValidCodeVerifier(``)).toBe(false)
  })
})
