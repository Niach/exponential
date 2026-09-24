import { describe, expect, it } from "vitest"
import { authErrorMessage } from "@/lib/auth/error-messages"

describe(`authErrorMessage`, () => {
  it(`maps Better Auth 1.6's sign-up duplicate code to the sign-in-instead copy (EXP-630)`, () => {
    // sign-up.mjs throws USER_ALREADY_EXISTS_USE_ANOTHER_EMAIL; an invited
    // placeholder's address hits it, and the person must sign in instead.
    const message = authErrorMessage(
      { code: `USER_ALREADY_EXISTS_USE_ANOTHER_EMAIL`, message: `raw` },
      `fallback`
    )
    expect(message).toMatch(/sign in instead/i)
    expect(message).toBe(
      authErrorMessage({ code: `USER_ALREADY_EXISTS` }, `fallback`)
    )
  })

  it(`falls back to the server message, then the caller's fallback`, () => {
    expect(authErrorMessage({ code: `NOPE`, message: `raw` }, `fb`)).toBe(`raw`)
    expect(authErrorMessage({ code: `NOPE` }, `fb`)).toBe(`fb`)
    expect(authErrorMessage(null, `fb`)).toBe(`fb`)
  })
})
