// EXP-1490: the vault, encryption at rest for every secret column eMCP
// touches (better-auth `accounts` tokens, `mcp_credentials`). Byte-locked by
// `fixtures/vault.json` (its `$comment` is the spec, its vectors the proof).

import { createCipheriv, createDecipheriv, randomBytes } from "node:crypto"

import fixture from "../fixtures/vault.json" with { type: "json" }

export const VAULT_VERSION: string = fixture.version

export interface VaultKey {
  kid: string
  key: Buffer
}

/** The current key and the retired ones (decrypt only). */
export interface Keyring {
  current: VaultKey
  retired: readonly VaultKey[]
}

const KID = new RegExp(fixture.kidPattern)
const IV_BYTES = fixture.ivBytes
const TAG_BYTES = fixture.tagBytes

/** `<kid>:<base64 of 32 bytes>` → the key; null for anything else. */
export function parseVaultKey(spec: string | null | undefined): VaultKey | null {
  if (!spec) return null
  const at = spec.indexOf(`:`)
  if (at <= 0) return null
  const kid = spec.slice(0, at)
  const text = spec.slice(at + 1).trim()
  if (!KID.test(kid) || !/^[A-Za-z0-9+/]+={0,2}$/.test(text)) return null
  const key = Buffer.from(text, `base64`)
  return key.length === fixture.keyBytes ? { kid, key } : null
}

/** `VAULT_KEY` + `VAULT_KEYS_RETIRED` (comma-separated) → the keyring;
 *  null when the current key is missing or malformed, or a kid repeats. A
 *  malformed retired entry is null too: a typo must not lose a key. */
export function parseKeyring(current: string | null | undefined, retired: string | null | undefined): Keyring | null {
  const key = parseVaultKey(current)
  if (!key) return null
  const old: VaultKey[] = []
  for (const part of (retired ?? ``).split(`,`).map((p) => p.trim()).filter(Boolean)) {
    const parsed = parseVaultKey(part)
    if (!parsed) return null
    old.push(parsed)
  }
  const kids = [key.kid, ...old.map((k) => k.kid)]
  if (new Set(kids).size !== kids.length) return null
  return { current: key, retired: old }
}

/** The additional data binding a value to its row and the key that sealed
 *  it: `ev1|<kid>|<table>.<column>|<row id>`. */
export function vaultAad(kid: string, table: string, column: string, rowId: string): string {
  return [fixture.version, kid, `${table}.${column}`, rowId].join(`|`)
}

export interface VaultSlot {
  table: string
  column: string
  rowId: string
}

/** Seal with an explicit iv (the vectors); `vaultSeal` draws one. */
export function vaultSealWith(key: VaultKey, iv: Buffer, plaintext: string, slot: VaultSlot): string {
  const cipher = createCipheriv(`aes-256-gcm`, key.key, iv)
  cipher.setAAD(Buffer.from(vaultAad(key.kid, slot.table, slot.column, slot.rowId), `utf8`))
  const body = Buffer.concat([cipher.update(plaintext, `utf8`), cipher.final(), cipher.getAuthTag()])
  return [fixture.version, key.kid, iv.toString(`base64url`), body.toString(`base64url`)].join(`.`)
}

export function vaultSeal(keyring: Keyring, plaintext: string, slot: VaultSlot): string {
  return vaultSealWith(keyring.current, randomBytes(IV_BYTES), plaintext, slot)
}

/** The kid a value was sealed with; null for anything not a vault value. */
export function vaultKid(value: string | null | undefined): string | null {
  const parts = (value ?? ``).split(`.`)
  return parts.length === 4 && parts[0] === fixture.version && KID.test(parts[1]!) ? parts[1]! : null
}

/** The plaintext, or null for anything that does not open: another
 *  version, an unknown kid, another slot, a tampered value. */
export function vaultOpen(keyring: Keyring, value: string | null | undefined, slot: VaultSlot): string | null {
  const kid = vaultKid(value)
  if (kid === null) return null
  const key = [keyring.current, ...keyring.retired].find((k) => k.kid === kid)
  if (!key) return null
  const [, , ivText, bodyText] = value!.split(`.`)
  try {
    const iv = Buffer.from(ivText!, `base64url`)
    const body = Buffer.from(bodyText!, `base64url`)
    if (iv.length !== IV_BYTES || body.length < TAG_BYTES) return null
    const decipher = createDecipheriv(`aes-256-gcm`, key.key, iv)
    decipher.setAAD(Buffer.from(vaultAad(kid, slot.table, slot.column, slot.rowId), `utf8`))
    decipher.setAuthTag(body.subarray(body.length - TAG_BYTES))
    return Buffer.concat([decipher.update(body.subarray(0, body.length - TAG_BYTES)), decipher.final()]).toString(`utf8`)
  } catch {
    return null
  }
}

/** A value sealed with a retired key: the rotation sweep re-seals it. */
export function vaultNeedsRotation(keyring: Keyring, value: string | null | undefined): boolean {
  const kid = vaultKid(value)
  return kid !== null && kid !== keyring.current.kid
}
