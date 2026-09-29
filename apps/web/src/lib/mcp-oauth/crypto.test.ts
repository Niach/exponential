import { afterEach, describe, expect, it } from "vitest"

// MCP credentials at rest: AES-256-GCM under an HKDF key off
// BETTER_AUTH_SECRET. A rotated secret, a tampered value or junk all read
// as null (= "not connected"), never throw.

import {
  credentialAad,
  decryptCredential,
  decryptSecret,
  encryptCredential,
  encryptSecret,
} from "@/lib/mcp-oauth/crypto"

const AAD = credentialAad(`server-1`, `user-1`)
const ORIGINAL = process.env.BETTER_AUTH_SECRET
afterEach(() => {
  process.env.BETTER_AUTH_SECRET = ORIGINAL
})

describe(`mcp credential crypto`, () => {
  it(`round-trips, in the v1.<iv>.<ct+tag> format, with a fresh iv each time`, () => {
    process.env.BETTER_AUTH_SECRET = `test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaaa`
    const a = encryptSecret(`hunter2`, AAD)
    const b = encryptSecret(`hunter2`, AAD)
    expect(a).toMatch(/^v1\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/)
    expect(a).not.toBe(b)
    expect(a).not.toContain(`hunter2`)
    expect(decryptSecret(a, AAD)).toBe(`hunter2`)
    expect(decryptSecret(b, AAD)).toBe(`hunter2`)
  })

  it(`round-trips a credential payload`, () => {
    process.env.BETTER_AUTH_SECRET = `test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaaa`
    const payload = { accessToken: `at`, refreshToken: `rt`, tokenType: `Bearer` }
    expect(decryptCredential(encryptCredential(payload, AAD), AAD)).toEqual(payload)
  })

  it(`a rotated BETTER_AUTH_SECRET reads as null`, () => {
    process.env.BETTER_AUTH_SECRET = `test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaaa`
    const sealed = encryptCredential({ value: `k` }, AAD)
    process.env.BETTER_AUTH_SECRET = `test-secret-bbbbbbbbbbbbbbbbbbbbbbbbbbbb`
    expect(decryptCredential(sealed, AAD)).toBeNull()
  })

  it(`a tampered or malformed value reads as null`, () => {
    process.env.BETTER_AUTH_SECRET = `test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaaa`
    const sealed = encryptSecret(`value`, AAD)
    const [v, iv, ct] = sealed.split(`.`)
    const flipped = `${ct!.slice(0, -2)}${ct!.endsWith(`A`) ? `B` : `A`}${ct!.slice(-1)}`
    expect(decryptSecret(`${v}.${iv}.${flipped}`, AAD)).toBeNull()
    expect(decryptSecret(`v2.${iv}.${ct}`, AAD)).toBeNull()
    expect(decryptSecret(`garbage`, AAD)).toBeNull()
    expect(decryptSecret(null, AAD)).toBeNull()
    expect(decryptCredential(encryptSecret(`not json`, AAD), AAD)).toBeNull()
  })

  it(`a value sealed for another row (another AAD) reads as null`, () => {
    process.env.BETTER_AUTH_SECRET = `test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaaa`
    const sealed = encryptCredential({ accessToken: `at` }, AAD)
    expect(decryptCredential(sealed, credentialAad(`server-1`, `user-2`))).toBeNull()
    expect(decryptCredential(sealed, credentialAad(`server-2`, `user-1`))).toBeNull()
    expect(decryptCredential(sealed, AAD)).toEqual({ accessToken: `at` })
  })
})
