import { buildAppleRevokeBody, getAppleClientSecret } from "./apple"

// Revoke a user's Sign in with Apple pairing when their account is deleted.
// App Store guideline 5.1.1(v) requires apps that offer account deletion to
// revoke the SIWA tokens too — otherwise the pairing lives on in the user's
// Apple ID and a fresh signup still gets no name payload (Apple only re-sends
// the name after the pairing is revoked).
//
// Only WEB-flow accounts carry tokens to revoke: Better Auth stores
// access_token/refresh_token from the OAuth code exchange. Accounts created via
// the native idToken exchange store NO Apple tokens, so there is nothing to
// revoke for them — those users clear the pairing manually via Settings → Apple
// ID → Sign-In & Security → Exponential → Stop Using Apple ID.
//
// The capture side is provider-agnostic and lives in oauth-revocation.ts
// (REV2-76: Google and generic OIDC grants must be revoked too); this module is
// only the Apple-specific request.

export interface AppleTokenRow {
  accessToken: string | null
  refreshToken: string | null
}

// Best-effort revoke, mirroring cancelCreemSubscriptionsBestEffort: never
// throws (a delete must never be blocked by Apple being unreachable), logs
// loudly on failure. No-op when Apple isn't configured or no tokens were
// captured (native-idToken accounts, non-Apple users).
export async function revokeAppleTokensBestEffort(
  tokens: AppleTokenRow[]
): Promise<void> {
  if (tokens.length === 0) return
  const clientId = process.env.APPLE_CLIENT_ID
  const clientSecret = getAppleClientSecret()
  if (!clientId || !clientSecret) return

  for (const row of tokens) {
    const refresh = row.refreshToken?.trim()
    const access = row.accessToken?.trim()
    // Apple accepts either; prefer the refresh token, which revokes the whole
    // grant. A native-paired row has neither → nothing to revoke.
    const token = refresh || access
    if (!token) continue
    const tokenTypeHint = refresh ? `refresh_token` : `access_token`
    try {
      const res = await fetch(`https://appleid.apple.com/auth/revoke`, {
        method: `POST`,
        headers: { "content-type": `application/x-www-form-urlencoded` },
        body: buildAppleRevokeBody({
          clientId,
          clientSecret,
          token,
          tokenTypeHint,
        }),
      })
      if (!res.ok) {
        const detail = await res.text().catch(() => ``)
        console.error(
          `[apple-revoke] revoke failed: ${res.status} ${detail}`.trim()
        )
      }
    } catch (err) {
      console.error(`[apple-revoke] revoke request threw:`, err)
    }
  }
}
