import { createPrivateKey, sign as cryptoSign } from "node:crypto"

// Sign in with Apple client-secret minting, shared between the auth config
// (lib/auth/index.ts) and the account-deletion token-revocation helper
// (lib/auth/apple-revocation.ts). The client secret is an ES256 JWT that Apple
// hard-caps at 6 months, so instead of a static APPLE_CLIENT_SECRET that
// someone must re-mint twice a year, the server mints it from the SIWA .p8 key
// (APPLE_PRIVATE_KEY, base64 like GITHUB_APP_PRIVATE_KEY) — every call refreshes
// it. An explicit APPLE_CLIENT_SECRET still wins when set. Caveat: a container
// left running >6 months without a restart will see Apple logins fail until it
// is restarted (index.ts mints once at boot).
export function mintAppleClientSecret(): string | undefined {
  const keyB64 = process.env.APPLE_PRIVATE_KEY
  const keyId = process.env.APPLE_KEY_ID
  const teamId = process.env.APPLE_TEAM_ID
  const clientId = process.env.APPLE_CLIENT_ID
  if (!keyB64 || !keyId || !teamId || !clientId) return undefined
  try {
    const key = createPrivateKey(Buffer.from(keyB64, `base64`).toString(`utf8`))
    const b64u = (input: string | Buffer) =>
      Buffer.from(input).toString(`base64url`)
    const now = Math.floor(Date.now() / 1000)
    const data = `${b64u(JSON.stringify({ alg: `ES256`, kid: keyId }))}.${b64u(
      JSON.stringify({
        iss: teamId,
        iat: now,
        exp: now + 180 * 24 * 60 * 60, // Apple's maximum is 6 months
        aud: `https://appleid.apple.com`,
        sub: clientId,
      })
    )}`
    const sig = cryptoSign(`sha256`, Buffer.from(data), {
      key,
      dsaEncoding: `ieee-p1363`,
    })
    return `${data}.${b64u(sig)}`
  } catch (err) {
    console.error(
      `[auth] failed to mint the Apple client secret from APPLE_PRIVATE_KEY:`,
      err
    )
    return undefined
  }
}

// The effective Apple client secret: an explicit APPLE_CLIENT_SECRET wins,
// otherwise mint one from the .p8 key. Undefined when Apple isn't configured.
export function getAppleClientSecret(): string | undefined {
  return process.env.APPLE_CLIENT_SECRET || mintAppleClientSecret()
}

// The x-www-form-urlencoded body Apple's /auth/revoke endpoint expects. Lives
// here (crypto-only module, no DB imports) so it stays a pure, unit-testable
// seam; the account-deletion revocation flow is in apple-revocation.ts.
export function buildAppleRevokeBody(params: {
  clientId: string
  clientSecret: string
  token: string
  tokenTypeHint: `refresh_token` | `access_token`
}): string {
  const body = new URLSearchParams()
  body.set(`client_id`, params.clientId)
  body.set(`client_secret`, params.clientSecret)
  body.set(`token`, params.token)
  body.set(`token_type_hint`, params.tokenTypeHint)
  return body.toString()
}
