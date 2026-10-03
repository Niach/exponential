import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { createHmac, randomUUID } from "crypto"
import { mintReporterToken, verifyReporterToken } from "./token"

// The magic-link token is the reporter's only credential — these lock the
// deterministic HMAC contract (EXP-132, over the issue id since SLOP-4):
// mint/verify round-trip, stability (the same issue always yields the same
// link), and rejection of anything forged, tampered, minted under a
// different secret or under the retired thread context.

const SECRET = `test-secret-test-secret-test-secret!`

beforeEach(() => {
  process.env.BETTER_AUTH_SECRET = SECRET
})

afterEach(() => {
  process.env.BETTER_AUTH_SECRET = SECRET
})

describe(`mintReporterToken`, () => {
  it(`round-trips through verifyReporterToken`, () => {
    const issueId = randomUUID()
    expect(verifyReporterToken(mintReporterToken(issueId))).toBe(issueId)
  })

  it(`is deterministic — the same issue always gets the same link`, () => {
    const issueId = randomUUID()
    expect(mintReporterToken(issueId)).toBe(mintReporterToken(issueId))
  })

  it(`emits <uuid>.<43-char base64url mac>`, () => {
    const issueId = randomUUID()
    const token = mintReporterToken(issueId)
    const [id, mac, rest] = token.split(`.`)
    expect(id).toBe(issueId)
    expect(mac).toMatch(/^[A-Za-z0-9_-]{43}$/)
    expect(rest).toBeUndefined()
  })

  it(`mints distinct tokens for distinct issues`, () => {
    expect(mintReporterToken(randomUUID())).not.toBe(
      mintReporterToken(randomUUID())
    )
  })

  it(`throws without a server secret`, () => {
    delete process.env.BETTER_AUTH_SECRET
    expect(() => mintReporterToken(randomUUID())).toThrow()
  })
})

describe(`verifyReporterToken`, () => {
  it(`rejects malformed tokens`, () => {
    expect(verifyReporterToken(``)).toBeNull()
    expect(verifyReporterToken(`abc`)).toBeNull()
    expect(verifyReporterToken(randomUUID())).toBeNull()
    expect(verifyReporterToken(`A`.repeat(43))).toBeNull()
  })

  it(`rejects a tampered mac`, () => {
    const token = mintReporterToken(randomUUID())
    const flipped = token.slice(0, -1) + (token.endsWith(`A`) ? `B` : `A`)
    expect(verifyReporterToken(flipped)).toBeNull()
  })

  it(`rejects a mac transplanted onto another issue id`, () => {
    const mac = mintReporterToken(randomUUID()).split(`.`)[1]
    expect(verifyReporterToken(`${randomUUID()}.${mac}`)).toBeNull()
  })

  it(`rejects a pre-SLOP-4 thread token (different HMAC context)`, () => {
    const id = randomUUID()
    const legacyMac = createHmac(`sha256`, SECRET)
      .update(`exp-support-thread:v1:${id}`)
      .digest(`base64url`)
    expect(verifyReporterToken(`${id}.${legacyMac}`)).toBeNull()
  })

  it(`rejects tokens minted under a different secret`, () => {
    const token = mintReporterToken(randomUUID())
    process.env.BETTER_AUTH_SECRET = `another-secret-another-secret-another!`
    expect(verifyReporterToken(token)).toBeNull()
  })

  it(`rejects everything when the server secret is unset`, () => {
    const token = mintReporterToken(randomUUID())
    delete process.env.BETTER_AUTH_SECRET
    expect(verifyReporterToken(token)).toBeNull()
  })
})
