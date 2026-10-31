// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/vault.json" with { type: "json" }
import { parseKeyring, parseVaultKey, vaultAad, vaultNeedsRotation, vaultOpen, vaultSeal, vaultSealWith } from "./vault"

const keyring = parseKeyring(fixture.keys.k1, fixture.keys.k0)!

describe(`parseKeyring`, () => {
  for (const entry of fixture.keyringCases) {
    test(entry.name, () => {
      const parsed = parseKeyring(entry.current, entry.retired)
      expect(parsed && { current: parsed.current.kid, retired: parsed.retired.map((k) => k.kid) }).toEqual(entry.expected)
    })
  }
})

describe(`vaultAad`, () => {
  for (const entry of fixture.aadCases) {
    test(entry.expected, () => expect(vaultAad(entry.kid, entry.table, entry.column, entry.rowId)).toBe(entry.expected))
  }
})

describe(`seal vectors`, () => {
  fixture.sealVectors.forEach((vector, i) => {
    test(`vector ${i}`, () => {
      const key = parseVaultKey(fixture.keys[vector.key as `k1` | `k0`])!
      expect(vaultSealWith(key, Buffer.from(vector.iv, `base64url`), vector.plaintext, vector.slot)).toBe(vector.expected)
    })
  })
  test(`a random iv round-trips`, () => {
    const slot = { table: `accounts`, column: `access_token`, rowId: `acc_9` }
    const a = vaultSeal(keyring, `x`, slot)
    expect(a).not.toBe(vaultSeal(keyring, `x`, slot))
    expect(vaultOpen(keyring, a, slot)).toBe(`x`)
  })
})

describe(`vaultOpen`, () => {
  for (const entry of fixture.openCases) {
    test(entry.name, () => {
      let value = fixture.sealVectors[entry.vector]!.expected
      if (entry.replace) value = value.replace(entry.replace[0]!, entry.replace[1]!)
      if (entry.tamperLast) value = value.slice(0, -1) + (value.endsWith(`A`) ? `B` : `A`)
      expect(vaultOpen(keyring, value, entry.slot)).toBe(entry.expected)
    })
  }
  for (const value of fixture.malformedCases) {
    test(`malformed ${JSON.stringify(value)}`, () => {
      expect(vaultOpen(keyring, value, { table: `accounts`, column: `access_token`, rowId: `acc_1` })).toBeNull()
    })
  }
})

describe(`vaultNeedsRotation`, () => {
  for (const entry of fixture.rotationCases) {
    test(JSON.stringify(entry), () => {
      const value = entry.value ?? fixture.sealVectors[entry.vector!]!.expected
      expect(vaultNeedsRotation(keyring, value)).toBe(entry.expected)
    })
  }
})
