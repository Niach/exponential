// Team MCP server credentials at rest (`mcp_credentials`), sealed by the
// vault (../vault.ts, #EXP-1491): `ev1.<kid>.<iv>.<body>`, bound to the
// row's id. A row from before EXP-1491 (`v1.`, keyed off BETTER_AUTH_SECRET
// and bound to (server, member)) still opens until the vault sweep re-seals
// it. Anything that does not open reads as "not connected", never as an
// error.
import {
  credentialSlot,
  legacyCredentialAad,
  openSecret,
  sealSecret,
} from "@/lib/emcp/vault"

/** The decrypted JSON payload of one `mcp_credentials` row. */
export interface McpCredentialPayload {
  accessToken?: string
  refreshToken?: string
  tokenType?: string
  /** Where the refresh posts (non-secret, but kept beside the token it
   * refreshes rather than re-discovered on every launch). */
  tokenEndpoint?: string
  /** A typed secret (`auth = secret`). */
  value?: string
}

/** The columns that locate a credential row's seal. */
export interface CredentialRowRef {
  id: string
  serverId: string
  userId: string
}

export function encryptCredential(payload: McpCredentialPayload, id: string): string {
  return sealSecret(JSON.stringify(payload), credentialSlot(id))
}

/** The plaintext JSON of a row's ciphertext (null = does not open). */
export function openCredentialText(value: string | null | undefined, row: CredentialRowRef): string | null {
  return openSecret(value, credentialSlot(row.id), legacyCredentialAad(row.serverId, row.userId))
}

export function decryptCredential(
  value: string | null | undefined,
  row: CredentialRowRef
): McpCredentialPayload | null {
  const text = openCredentialText(value, row)
  if (text === null) return null
  try {
    const parsed: unknown = JSON.parse(text)
    return parsed && typeof parsed === `object` && !Array.isArray(parsed)
      ? (parsed as McpCredentialPayload)
      : null
  } catch {
    return null
  }
}
