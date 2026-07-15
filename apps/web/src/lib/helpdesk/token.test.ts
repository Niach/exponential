import { describe, expect, it } from "vitest"
import {
  generateSupportToken,
  isValidSupportTokenShape,
  supportTokensMatch,
} from "./token"

// The magic-link token is the reporter's only credential — these lock the
// shape contract (validation gate for the anonymous endpoints) and the
// constant-time comparison path.

describe(`generateSupportToken`, () => {
  it(`emits 43-char base64url tokens that pass the shape gate`, () => {
    for (let i = 0; i < 20; i++) {
      const token = generateSupportToken()
      expect(token).toHaveLength(43)
      expect(isValidSupportTokenShape(token)).toBe(true)
    }
  })

  it(`emits unique tokens`, () => {
    const seen = new Set(
      Array.from({ length: 100 }, () => generateSupportToken())
    )
    expect(seen.size).toBe(100)
  })
})

describe(`isValidSupportTokenShape`, () => {
  it(`rejects wrong lengths`, () => {
    expect(isValidSupportTokenShape(``)).toBe(false)
    expect(isValidSupportTokenShape(`abc`)).toBe(false)
    expect(isValidSupportTokenShape(`${generateSupportToken()}x`)).toBe(false)
  })

  it(`rejects characters outside the base64url alphabet`, () => {
    const almost = generateSupportToken().slice(0, 42)
    expect(isValidSupportTokenShape(`${almost}=`)).toBe(false)
    expect(isValidSupportTokenShape(`${almost}/`)).toBe(false)
    expect(isValidSupportTokenShape(`${almost}+`)).toBe(false)
  })
})

describe(`supportTokensMatch`, () => {
  it(`matches an identical token`, () => {
    const token = generateSupportToken()
    expect(supportTokensMatch(token, token)).toBe(true)
  })

  it(`rejects a different token`, () => {
    expect(
      supportTokensMatch(generateSupportToken(), generateSupportToken())
    ).toBe(false)
  })

  it(`rejects length mismatches without throwing`, () => {
    const token = generateSupportToken()
    expect(supportTokensMatch(token, token.slice(0, 20))).toBe(false)
    expect(supportTokensMatch(token, ``)).toBe(false)
  })
})
