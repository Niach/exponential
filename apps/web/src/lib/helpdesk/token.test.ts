import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { randomUUID } from "crypto"
import { mintSupportToken, verifySupportToken } from "./token"

// The magic-link token is the reporter's only credential — these lock the
// deterministic HMAC contract (EXP-132): mint/verify round-trip, stability
// (the same thread always yields the same link), and rejection of anything
// forged, tampered, or minted under a different secret.

const SECRET = `test-secret-test-secret-test-secret!`

beforeEach(() => {
  process.env.BETTER_AUTH_SECRET = SECRET
})

afterEach(() => {
  process.env.BETTER_AUTH_SECRET = SECRET
})

describe(`mintSupportToken`, () => {
  it(`round-trips through verifySupportToken`, () => {
    const threadId = randomUUID()
    expect(verifySupportToken(mintSupportToken(threadId))).toBe(threadId)
  })

  it(`is deterministic — the same thread always gets the same link`, () => {
    const threadId = randomUUID()
    expect(mintSupportToken(threadId)).toBe(mintSupportToken(threadId))
  })

  it(`emits <uuid>.<43-char base64url mac>`, () => {
    const threadId = randomUUID()
    const token = mintSupportToken(threadId)
    const [id, mac, rest] = token.split(`.`)
    expect(id).toBe(threadId)
    expect(mac).toMatch(/^[A-Za-z0-9_-]{43}$/)
    expect(rest).toBeUndefined()
  })

  it(`mints distinct tokens for distinct threads`, () => {
    expect(mintSupportToken(randomUUID())).not.toBe(
      mintSupportToken(randomUUID())
    )
  })

  it(`throws without a server secret`, () => {
    delete process.env.BETTER_AUTH_SECRET
    expect(() => mintSupportToken(randomUUID())).toThrow()
  })
})

describe(`verifySupportToken`, () => {
  it(`rejects malformed tokens`, () => {
    expect(verifySupportToken(``)).toBeNull()
    expect(verifySupportToken(`abc`)).toBeNull()
    expect(verifySupportToken(randomUUID())).toBeNull()
    // Pre-EXP-132 tokens were 43 bare base64url chars — must no longer pass.
    expect(verifySupportToken(`A`.repeat(43))).toBeNull()
  })

  it(`rejects a tampered mac`, () => {
    const token = mintSupportToken(randomUUID())
    const flipped = token.slice(0, -1) + (token.endsWith(`A`) ? `B` : `A`)
    expect(verifySupportToken(flipped)).toBeNull()
  })

  it(`rejects a mac transplanted onto another thread id`, () => {
    const mac = mintSupportToken(randomUUID()).split(`.`)[1]
    expect(verifySupportToken(`${randomUUID()}.${mac}`)).toBeNull()
  })

  it(`rejects tokens minted under a different secret`, () => {
    const token = mintSupportToken(randomUUID())
    process.env.BETTER_AUTH_SECRET = `another-secret-another-secret-another!`
    expect(verifySupportToken(token)).toBeNull()
  })

  it(`rejects everything when the server secret is unset`, () => {
    const token = mintSupportToken(randomUUID())
    delete process.env.BETTER_AUTH_SECRET
    expect(verifySupportToken(token)).toBeNull()
  })
})
