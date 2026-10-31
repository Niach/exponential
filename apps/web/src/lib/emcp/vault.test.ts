import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { createCipheriv, hkdfSync, randomBytes } from "node:crypto"
import fixture from "@exp/emcp/fixtures/vault.json" with { type: "json" }

// #EXP-1491: the server vault replays @exp/emcp's vault.json through the
// environment (VAULT_KEY / VAULT_KEYS_RETIRED), and still opens what came
// before it: legacy `v1.` values and plaintext `accounts` tokens.
import {
  accountSlot,
  assertVaultConfigured,
  credentialSlot,
  DERIVED_KID,
  legacyCredentialAad,
  needsReseal,
  openAccountToken,
  openSecret,
  sealAccountToken,
  sealSecret,
  VaultConfigError,
  vaultKeyring,
} from "@/lib/emcp/vault"

const ENV = [`VAULT_KEY`, `VAULT_KEYS_RETIRED`, `BETTER_AUTH_SECRET`] as const
const saved: Record<string, string | undefined> = {}
const SECRET = `test-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaaa`

beforeEach(() => {
  for (const name of ENV) saved[name] = process.env[name]
  process.env.VAULT_KEY = fixture.keys.k1
  process.env.VAULT_KEYS_RETIRED = fixture.keys.k0
  process.env.BETTER_AUTH_SECRET = SECRET
})
afterEach(() => {
  for (const name of ENV) {
    if (saved[name] === undefined) delete process.env[name]
    else process.env[name] = saved[name]
  }
})

/** A pre-EXP-1491 `v1.` value, sealed the way lib/mcp-oauth/crypto.ts did. */
function legacySeal(plaintext: string, aad: string, secret = SECRET): string {
  const key = Buffer.from(hkdfSync(`sha256`, secret, `exp-mcp-credentials-v1`, `mcp-credentials`, 32))
  const iv = randomBytes(12)
  const cipher = createCipheriv(`aes-256-gcm`, key, iv)
  cipher.setAAD(Buffer.from(aad, `utf8`))
  const body = Buffer.concat([cipher.update(plaintext, `utf8`), cipher.final(), cipher.getAuthTag()])
  return `v1.${iv.toString(`base64url`)}.${body.toString(`base64url`)}`
}

describe(`vault.json replay (server keyring)`, () => {
  it(`opens every openCase`, () => {
    for (const entry of fixture.openCases) {
      let value = fixture.sealVectors[entry.vector]!.expected
      if (entry.replace) value = value.replace(entry.replace[0]!, entry.replace[1]!)
      if (entry.tamperLast) {
        const parts = value.split(`.`)
        const body = Buffer.from(parts[3]!, `base64url`)
        body[body.length - 1]! ^= 1
        parts[3] = body.toString(`base64url`)
        value = parts.join(`.`)
      }
      expect(openSecret(value, entry.slot), entry.name).toBe(entry.expected)
    }
  })

  it(`reads every malformed value as null`, () => {
    for (const value of fixture.malformedCases) {
      expect(openSecret(value, credentialSlot(`x`))).toBeNull()
    }
  })

  it(`flags the rotation cases`, () => {
    for (const entry of fixture.rotationCases) {
      const value = `vector` in entry ? fixture.sealVectors[entry.vector!]!.expected : entry.value!
      // A non-vault value is plaintext to the server: it needs a seal.
      const expected = `vector` in entry ? entry.expected : true
      expect(needsReseal(value)).toBe(expected)
    }
  })

  it(`seals under the current kid and opens only in the same slot`, () => {
    const sealed = sealSecret(`hunter2`, credentialSlot(`row-1`))
    expect(sealed.startsWith(`ev1.k1.`)).toBe(true)
    expect(openSecret(sealed, credentialSlot(`row-1`))).toBe(`hunter2`)
    expect(openSecret(sealed, credentialSlot(`row-2`))).toBeNull()
    expect(needsReseal(sealed)).toBe(false)
  })
})

describe(`the keyring from the environment`, () => {
  it(`keeps the BETTER_AUTH_SECRET-derived key as an opener once VAULT_KEY is set`, () => {
    const ring = vaultKeyring()
    expect(ring.current.kid).toBe(`k1`)
    expect(ring.retired.map((k) => k.kid)).toEqual([`k0`, DERIVED_KID])
  })

  it(`without VAULT_KEY seals under the derived key, which then rotates to VAULT_KEY`, () => {
    delete process.env.VAULT_KEY
    delete process.env.VAULT_KEYS_RETIRED
    expect(assertVaultConfigured()).toEqual({ kid: DERIVED_KID, derived: true })
    const sealed = sealSecret(`v`, credentialSlot(`r`))
    expect(sealed.startsWith(`ev1.${DERIVED_KID}.`)).toBe(true)
    process.env.VAULT_KEY = fixture.keys.k1
    expect(openSecret(sealed, credentialSlot(`r`))).toBe(`v`)
    expect(needsReseal(sealed)).toBe(true)
  })

  it(`refuses a malformed key instead of sealing under it`, () => {
    process.env.VAULT_KEY = `k1:nope`
    expect(() => assertVaultConfigured()).toThrow(VaultConfigError)
    process.env.VAULT_KEY = fixture.keys.k1
    process.env.VAULT_KEYS_RETIRED = `k0:nope`
    expect(() => sealSecret(`v`, credentialSlot(`r`))).toThrow(VaultConfigError)
    delete process.env.VAULT_KEY
    delete process.env.BETTER_AUTH_SECRET
    process.env.VAULT_KEYS_RETIRED = ``
    expect(() => assertVaultConfigured()).toThrow(VaultConfigError)
  })
})

describe(`legacy values`, () => {
  it(`opens a v1 value with its legacy AAD, and only with it`, () => {
    const legacy = legacySeal(`{"value":"k"}`, legacyCredentialAad(`s1`, `u1`))
    expect(openSecret(legacy, credentialSlot(`row`), legacyCredentialAad(`s1`, `u1`))).toBe(`{"value":"k"}`)
    expect(openSecret(legacy, credentialSlot(`row`), legacyCredentialAad(`s1`, `u2`))).toBeNull()
    expect(openSecret(legacy, credentialSlot(`row`))).toBeNull()
    expect(needsReseal(legacy)).toBe(true)
  })

  it(`a v1 value under another BETTER_AUTH_SECRET reads as null`, () => {
    const legacy = legacySeal(`x`, `aad`, `test-secret-bbbbbbbbbbbbbbbbbbbbbbbbbbbb`)
    expect(openSecret(legacy, credentialSlot(`row`), `aad`)).toBeNull()
  })
})

describe(`accounts tokens`, () => {
  it(`seals a token bound to its row and column; plaintext reads as itself`, () => {
    const sealed = sealAccountToken(`gho_secret-token`, `acc_1`, `access_token`)!
    expect(sealed.startsWith(`ev1.k1.`)).toBe(true)
    expect(openAccountToken(sealed, `acc_1`, `access_token`)).toBe(`gho_secret-token`)
    expect(openAccountToken(sealed, `acc_1`, `refresh_token`)).toBeNull()
    expect(openAccountToken(sealed, `acc_2`, `access_token`)).toBeNull()
    expect(openAccountToken(`gho_plain`, `acc_1`, `access_token`)).toBe(`gho_plain`)
    expect(openAccountToken(null, `acc_1`, `access_token`)).toBeNull()
    expect(sealAccountToken(sealed, `acc_1`, `access_token`)).toBe(sealed)
    expect(sealAccountToken(null, `acc_1`, `access_token`)).toBeNull()
  })

  it(`opens the fixture's accounts vector`, () => {
    const vector = fixture.sealVectors[0]!
    expect(openSecret(vector.expected, accountSlot(`acc_1`, `access_token`))).toBe(vector.plaintext)
  })
})
