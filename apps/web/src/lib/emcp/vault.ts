// #EXP-1491: the vault, encryption at rest for every secret column eMCP
// touches: the better-auth `accounts` tokens, `mcp_credentials.ciphertext`,
// the flow's PKCE verifier and a DCR client's secret. The format, the AAD and
// the keyring are @exp/emcp's (`fixtures/vault.json`); this module binds them
// to the environment and to the rows.
//
// THE KEYRING: `VAULT_KEY` = `<kid>:<base64 of 32 bytes>` (the current key),
// `VAULT_KEYS_RETIRED` = old keys that still OPEN. A set but malformed
// `VAULT_KEY` refuses to boot (`assertVaultConfigured`). An instance without
// `VAULT_KEY` (every install before EXP-1491) seals under a key derived from
// BETTER_AUTH_SECRET with HKDF (kid `auth`), so nothing has to be provisioned
// to upgrade; once `VAULT_KEY` is set that derived key only opens, and the
// sweep (vault-sweep.ts) re-seals its values under the new kid.
//
// LEGACY VALUES: before EXP-1491 the MCP secrets were `v1.<iv>.<body>` under
// an HKDF key (salt `exp-mcp-credentials-v1`) with per-table AAD strings, and
// the `accounts` tokens were plaintext. `openSecret` still reads both (the
// caller passes the legacy AAD), and the sweep migrates them to `ev1`.
// Anything that does not open reads as null = "not connected", never an
// error.
import { createDecipheriv, hkdfSync } from "node:crypto"
import {
  parseKeyring,
  vaultKid,
  vaultNeedsRotation,
  vaultOpen,
  vaultSeal,
  type Keyring,
  type VaultKey,
  type VaultSlot,
} from "@exp/emcp/vault"

export type { VaultSlot } from "@exp/emcp/vault"

/** The kid of the key derived from BETTER_AUTH_SECRET. */
export const DERIVED_KID = `auth`
const DERIVED_SALT = `exp-vault-ev1`
const DERIVED_INFO = `vault`

const LEGACY_SALT = `exp-mcp-credentials-v1`
const LEGACY_INFO = `mcp-credentials`
const LEGACY_VERSION = `v1`
const IV_BYTES = 12
const TAG_BYTES = 16

export class VaultConfigError extends Error {}

function derive(secret: string, salt: string, info: string): Buffer {
  return Buffer.from(hkdfSync(`sha256`, secret, salt, info, 32))
}

interface Resolved {
  env: string
  keyring: Keyring
  legacy: Buffer | null
}

let cached: Resolved | null = null

/** The keyring for the current environment; throws VaultConfigError when no
 * key can be had (a malformed `VAULT_KEY` / `VAULT_KEYS_RETIRED`, or neither
 * `VAULT_KEY` nor BETTER_AUTH_SECRET). */
function resolve(): Resolved {
  const current = process.env.VAULT_KEY?.trim() || null
  const retired = process.env.VAULT_KEYS_RETIRED ?? null
  const secret = process.env.BETTER_AUTH_SECRET || null
  const env = JSON.stringify([current, retired, secret])
  if (cached?.env === env) return cached

  const derived: VaultKey | null = secret
    ? { kid: DERIVED_KID, key: derive(secret, DERIVED_SALT, DERIVED_INFO) }
    : null
  let keyring: Keyring
  if (current) {
    const parsed = parseKeyring(current, retired)
    if (!parsed) {
      throw new VaultConfigError(
        `VAULT_KEY / VAULT_KEYS_RETIRED is malformed: expected <kid>:<base64 of 32 bytes>, kids [a-z0-9]{1,16}, each kid once`
      )
    }
    const kids = new Set([parsed.current.kid, ...parsed.retired.map((k) => k.kid)])
    keyring =
      derived && !kids.has(DERIVED_KID)
        ? { current: parsed.current, retired: [...parsed.retired, derived] }
        : parsed
  } else {
    if (retired?.trim()) {
      throw new VaultConfigError(`VAULT_KEYS_RETIRED is set without VAULT_KEY`)
    }
    if (!derived) {
      throw new VaultConfigError(`neither VAULT_KEY nor BETTER_AUTH_SECRET is set: cannot seal secrets`)
    }
    keyring = { current: derived, retired: [] }
  }
  cached = {
    env,
    keyring,
    legacy: secret ? derive(secret, LEGACY_SALT, LEGACY_INFO) : null,
  }
  return cached
}

/** Boot check (server-bun.ts): a set but malformed key stops the server
 * instead of sealing under a key nobody configured. */
export function assertVaultConfigured(): { kid: string; derived: boolean } {
  const { keyring } = resolve()
  return { kid: keyring.current.kid, derived: keyring.current.kid === DERIVED_KID && !process.env.VAULT_KEY?.trim() }
}

export function vaultKeyring(): Keyring {
  return resolve().keyring
}

/** Seal `plaintext` into its slot under the current key. */
export function sealSecret(plaintext: string, slot: VaultSlot): string {
  return vaultSeal(resolve().keyring, plaintext, slot)
}

/** The plaintext of a sealed value, or null. `legacyAad` opens a pre-EXP-1491
 * `v1.` value (its old per-table AAD); without it a `v1.` value is null. */
export function openSecret(
  value: string | null | undefined,
  slot: VaultSlot,
  legacyAad?: string
): string | null {
  if (!value) return null
  if (vaultKid(value) !== null) return vaultOpen(resolve().keyring, value, slot)
  if (legacyAad === undefined || !value.startsWith(`${LEGACY_VERSION}.`)) return null
  return openLegacy(value, legacyAad)
}

/** Does this value need re-sealing under the current key (a retired kid, a
 * legacy `v1.` value, anything else not sealed by the vault)? */
export function needsReseal(value: string | null | undefined): boolean {
  if (!value) return false
  if (vaultKid(value) === null) return true
  return vaultNeedsRotation(resolve().keyring, value)
}

/** Is this a vault value (any kid)? */
export function isSealed(value: string | null | undefined): boolean {
  return vaultKid(value) !== null
}

function openLegacy(value: string, aad: string): string | null {
  const key = resolve().legacy
  if (!key) return null
  const [, ivText, sealedText, ...rest] = value.split(`.`)
  if (!ivText || !sealedText || rest.length > 0) return null
  try {
    const iv = Buffer.from(ivText, `base64url`)
    const sealed = Buffer.from(sealedText, `base64url`)
    if (iv.length !== IV_BYTES || sealed.length < TAG_BYTES) return null
    const decipher = createDecipheriv(`aes-256-gcm`, key, iv)
    decipher.setAAD(Buffer.from(aad, `utf8`))
    decipher.setAuthTag(sealed.subarray(sealed.length - TAG_BYTES))
    return Buffer.concat([
      decipher.update(sealed.subarray(0, sealed.length - TAG_BYTES)),
      decipher.final(),
    ]).toString(`utf8`)
  } catch {
    return null
  }
}

// ── The slots ───────────────────────────────────────────────────────────

export function credentialSlot(id: string): VaultSlot {
  return { table: `mcp_credentials`, column: `ciphertext`, rowId: id }
}

export function flowVerifierSlot(id: string): VaultSlot {
  return { table: `mcp_oauth_flows`, column: `code_verifier_ciphertext`, rowId: id }
}

export function clientSecretSlot(id: string): VaultSlot {
  return { table: `mcp_oauth_clients`, column: `client_secret_ciphertext`, rowId: id }
}

export const ACCOUNT_TOKEN_COLUMNS = [`access_token`, `refresh_token`, `id_token`] as const
export type AccountTokenColumn = (typeof ACCOUNT_TOKEN_COLUMNS)[number]

export function accountSlot(id: string, column: AccountTokenColumn): VaultSlot {
  return { table: `accounts`, column, rowId: id }
}

// The pre-EXP-1491 AAD strings, read only to open legacy `v1.` values.
export function legacyCredentialAad(serverId: string, userId: string): string {
  return `mcp_credentials:${serverId}:${userId}`
}

export function legacyFlowVerifierAad(state: string): string {
  return `mcp_oauth_flows:${state}`
}

export function legacyClientSecretAad(issuer: string, registrationEndpoint: string): string {
  return `mcp_oauth_clients:${issuer}:${registrationEndpoint}`
}

// ── accounts tokens ─────────────────────────────────────────────────────

/** A better-auth `accounts` token as stored → the token. A value the vault
 * did not seal is a plaintext token from before EXP-1491 (the sweep seals
 * it) and reads as itself; a sealed one that does not open reads as null. */
export function openAccountToken(
  value: string | null | undefined,
  id: string,
  column: AccountTokenColumn
): string | null {
  if (value === null || value === undefined) return null
  if (!isSealed(value)) return value
  return openSecret(value, accountSlot(id, column))
}

/** A token → the stored value (null and empty stay as they are). */
export function sealAccountToken(
  value: string | null | undefined,
  id: string,
  column: AccountTokenColumn
): string | null | undefined {
  if (!value) return value
  if (isSealed(value)) return value
  return sealSecret(value, accountSlot(id, column))
}
