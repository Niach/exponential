import { describe, expect, it } from "vitest"
import {
  DEEP_LINK_SCHEME,
  OAUTH_ERROR_FALLBACK,
  githubConnectedDeepLink,
  normalizeOauthErrorReason,
  oauthErrorMessage,
  oauthReturnCodeDeepLink,
  oauthReturnErrorDeepLink,
} from "@/lib/deep-link"

describe(`success deep links`, () => {
  it(`doubles the code into query and fragment`, () => {
    expect(oauthReturnCodeDeepLink(`abc123`)).toBe(
      `${DEEP_LINK_SCHEME}://oauth-return?code=abc123#code=abc123`
    )
  })

  it(`percent-encodes the code in both forms`, () => {
    expect(oauthReturnCodeDeepLink(`a b+c`)).toBe(
      `${DEEP_LINK_SCHEME}://oauth-return?code=a%20b%2Bc#code=a%20b%2Bc`
    )
  })

  it(`mints the payloadless github link`, () => {
    expect(githubConnectedDeepLink()).toBe(
      `${DEEP_LINK_SCHEME}://github-connected`
    )
  })
})

describe(`oauthReturnErrorDeepLink`, () => {
  it(`doubles the reason into query and fragment`, () => {
    expect(oauthReturnErrorDeepLink(`access_denied`)).toBe(
      `${DEEP_LINK_SCHEME}://oauth-return?error=access_denied#error=access_denied`
    )
  })

  it(`normalizes a hostile upstream reason before it reaches the link`, () => {
    expect(oauthReturnErrorDeepLink(`Bad Thing<script>`)).toBe(
      `${DEEP_LINK_SCHEME}://oauth-return?error=bad_thing_script#error=bad_thing_script`
    )
  })
})

describe(`normalizeOauthErrorReason`, () => {
  it(`keeps a plain slug`, () => {
    expect(normalizeOauthErrorReason(`state_missing`)).toBe(`state_missing`)
  })

  it(`lowercases and collapses separators`, () => {
    expect(normalizeOauthErrorReason(`  Invalid Code!! `)).toBe(`invalid_code`)
  })

  it(`falls back for non-strings, blanks and pure punctuation`, () => {
    expect(normalizeOauthErrorReason(undefined)).toBe(OAUTH_ERROR_FALLBACK)
    expect(normalizeOauthErrorReason(42)).toBe(OAUTH_ERROR_FALLBACK)
    expect(normalizeOauthErrorReason(``)).toBe(OAUTH_ERROR_FALLBACK)
    expect(normalizeOauthErrorReason(`***`)).toBe(OAUTH_ERROR_FALLBACK)
  })

  it(`clamps long reasons to a short slug`, () => {
    expect(normalizeOauthErrorReason(`x`.repeat(200))).toHaveLength(48)
  })
})

describe(`oauthErrorMessage`, () => {
  it(`names the cancellation case`, () => {
    expect(oauthErrorMessage(`access_denied`)).toMatch(/cancelled/i)
  })

  it(`treats every expired-state variant the same`, () => {
    const expired = oauthErrorMessage(`state_missing`)
    expect(oauthErrorMessage(`state_invalid`)).toBe(expired)
    expect(oauthErrorMessage(`state_mismatch`)).toBe(expired)
    expect(oauthErrorMessage(`please_restart_the_process`)).toBe(expired)
  })

  it(`falls back for unknown and legacy reasons`, () => {
    const fallback = oauthErrorMessage(OAUTH_ERROR_FALLBACK)
    expect(oauthErrorMessage(`mobile_oauth_failed`)).toBe(fallback)
    expect(oauthErrorMessage(undefined)).toBe(fallback)
  })
})
