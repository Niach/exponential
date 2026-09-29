// Encryption at rest for team MCP server credentials (`mcp_credentials`,
// the PKCE verifier on `mcp_oauth_flows`, a DCR client secret). AES-256-GCM
// under a key derived from BETTER_AUTH_SECRET with HKDF-SHA256 (salt
// `exp-mcp-credentials-v1`), so no second secret to provision. Format:
// `v1.<iv b64url>.<ciphertext+tag b64url>`. Every value is BOUND to where
// it lives by GCM additional data (the `*Aad` helpers below), so a
// ciphertext copied into another member's / server's / flow's row fails to
// decrypt. Rotating BETTER_AUTH_SECRET makes every stored value
// undecryptable, which callers read as "not connected" (members reconnect)
// — never as an error.
import { createCipheriv, createDecipheriv, hkdfSync, randomBytes } from "node:crypto"

const SALT = `exp-mcp-credentials-v1`
const INFO = `mcp-credentials`
const VERSION = `v1`
const IV_BYTES = 12
const TAG_BYTES = 16

let cached: { secret: string; key: Buffer } | null = null

function key(): Buffer {
  const secret = process.env.BETTER_AUTH_SECRET
  if (!secret) {
    throw new Error(`BETTER_AUTH_SECRET is not set — cannot encrypt MCP credentials`)
  }
  if (cached?.secret !== secret) {
    cached = {
      secret,
      key: Buffer.from(hkdfSync(`sha256`, secret, SALT, INFO, 32)),
    }
  }
  return cached.key
}

/** The AAD of one `mcp_credentials` row. */
export function credentialAad(serverId: string, userId: string): string {
  return `mcp_credentials:${serverId}:${userId}`
}

/** The AAD of a flow's PKCE verifier. */
export function flowVerifierAad(state: string): string {
  return `mcp_oauth_flows:${state}`
}

/** The AAD of a cached DCR client's secret. */
export function clientSecretAad(issuer: string, registrationEndpoint: string): string {
  return `mcp_oauth_clients:${issuer}:${registrationEndpoint}`
}

export function encryptSecret(plaintext: string, aad: string): string {
  const iv = randomBytes(IV_BYTES)
  const cipher = createCipheriv(`aes-256-gcm`, key(), iv)
  cipher.setAAD(Buffer.from(aad, `utf8`))
  const body = Buffer.concat([cipher.update(plaintext, `utf8`), cipher.final()])
  const sealed = Buffer.concat([body, cipher.getAuthTag()])
  return `${VERSION}.${iv.toString(`base64url`)}.${sealed.toString(`base64url`)}`
}

/** The plaintext, or null for anything that does not decrypt under the
 * current key and `aad` (a rotated secret, a tampered value, one sealed for
 * another row). */
export function decryptSecret(
  value: string | null | undefined,
  aad: string
): string | null {
  if (!value) return null
  const [version, ivText, sealedText, ...rest] = value.split(`.`)
  if (version !== VERSION || !ivText || !sealedText || rest.length > 0) return null
  try {
    const iv = Buffer.from(ivText, `base64url`)
    const sealed = Buffer.from(sealedText, `base64url`)
    if (iv.length !== IV_BYTES || sealed.length < TAG_BYTES) return null
    const decipher = createDecipheriv(`aes-256-gcm`, key(), iv)
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

export function encryptCredential(payload: McpCredentialPayload, aad: string): string {
  return encryptSecret(JSON.stringify(payload), aad)
}

export function decryptCredential(
  value: string | null | undefined,
  aad: string
): McpCredentialPayload | null {
  const text = decryptSecret(value, aad)
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
