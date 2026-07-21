import { and, eq } from "drizzle-orm"
import { accounts } from "@/db/auth-schema"
import type { db as Database } from "@/db/connection"
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

// Works over the root db or a transaction — structurally typed so it can run
// wherever the caller needs it (only `.select` is used).
type DbOrTx = Pick<typeof Database, `select`>

export interface AppleTokenRow {
  accessToken: string | null
  refreshToken: string | null
}

// Capture the user's Apple `accounts` rows BEFORE the delete transaction — the
// user FK cascades on delete, after which the tokens are gone.
export async function captureAppleTokens(
  db: DbOrTx,
  userId: string
): Promise<AppleTokenRow[]> {
  return db
    .select({
      accessToken: accounts.accessToken,
      refreshToken: accounts.refreshToken,
    })
    .from(accounts)
    .where(and(eq(accounts.userId, userId), eq(accounts.providerId, `apple`)))
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
