import { describe, expect, it } from "vitest"
import {
  generateSupportToken,
  hashSupportToken,
  isValidSupportTokenShape,
  supportTokenHashMatches,
} from "./token"

// The magic-link token is the reporter's only credential — these lock the
// shape contract (validation gate for the anonymous endpoints) and the
// hash-then-compare lookup path.

describe(`generateSupportToken`, () => {
  it(`emits 43-char base64url tokens that pass the shape gate`, () => {
    for (let i = 0; i < 20; i++) {
      const token = generateSupportToken()
      expect(token).toHaveLength(43)
      expect(isValidSupportTokenShape(token)).toBe(true)
    }
  })

  it(`never repeats (32 bytes of entropy)`, () => {
    const seen = new Set(
      Array.from({ length: 100 }, () => generateSupportToken())
    )
    expect(seen.size).toBe(100)
  })
})

describe(`isValidSupportTokenShape`, () => {
  it(`rejects wrong lengths, padding and non-base64url characters`, () => {
    expect(isValidSupportTokenShape(``)).toBe(false)
    expect(isValidSupportTokenShape(`short`)).toBe(false)
    expect(isValidSupportTokenShape(`${generateSupportToken()}x`)).toBe(false)
    expect(isValidSupportTokenShape(`a`.repeat(42) + `=`)).toBe(false)
    expect(isValidSupportTokenShape(`a`.repeat(42) + `/`)).toBe(false)
    expect(isValidSupportTokenShape(`a`.repeat(42) + `+`)).toBe(false)
  })

  it(`accepts the full base64url alphabet at the right length`, () => {
    expect(isValidSupportTokenShape(`Az9_-`.repeat(8) + `Az9`)).toBe(true)
  })
})

describe(`supportTokenHashMatches`, () => {
  it(`matches a token against its own sha256`, () => {
    const token = generateSupportToken()
    expect(supportTokenHashMatches(token, hashSupportToken(token))).toBe(true)
  })

  it(`rejects a different token`, () => {
    const stored = hashSupportToken(generateSupportToken())
    expect(supportTokenHashMatches(generateSupportToken(), stored)).toBe(false)
  })

  it(`rejects malformed stored hashes without throwing`, () => {
    const token = generateSupportToken()
    expect(supportTokenHashMatches(token, ``)).toBe(false)
    expect(supportTokenHashMatches(token, `zz`)).toBe(false)
    expect(supportTokenHashMatches(token, `abcd`)).toBe(false)
  })

  it(`hashes are stable hex sha256`, () => {
    expect(hashSupportToken(`fixed-input`)).toBe(
      hashSupportToken(`fixed-input`)
    )
    expect(hashSupportToken(`fixed-input`)).toMatch(/^[0-9a-f]{64}$/)
  })
})
