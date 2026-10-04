import { and, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { accounts } from "@/db/auth-schema"
import { auth } from "@/lib/auth"
import { getAuthenticatedGithubUser } from "@/lib/integrations/github-app"

// SLOP-7: the user's GitHub connection IS their Better Auth `github` account
// row (the App's OAuth client, `linkSocial` for Google/Apple/code accounts).
// Better Auth stores the user-to-server token on that row and refreshes it
// through the provider when the App expires user tokens; this module is the
// one reader. Nothing else about the connection is persisted: installations
// and repositories are listed LIVE off GitHub with this token.

export const GITHUB_PROVIDER_ID = `github`

export interface GithubAccountRow {
  /** GitHub's numeric account id, as Better Auth stores it (a string). */
  accountId: string
  createdAt: Date
}

/** The caller's linked GitHub account, or null when none is linked. */
export async function githubAccountForUser(
  userId: string
): Promise<GithubAccountRow | null> {
  const [row] = await db
    .select({ accountId: accounts.accountId, createdAt: accounts.createdAt })
    .from(accounts)
    .where(
      and(eq(accounts.userId, userId), eq(accounts.providerId, GITHUB_PROVIDER_ID))
    )
    .limit(1)
  return row ?? null
}

export type GithubUserTokenState =
  | { state: `none` }
  | { state: `dead` }
  | { state: `ok`; token: string }

/**
 * The user's GitHub token, refreshed by Better Auth when the App expires user
 * tokens. `none` = no GitHub account linked; `dead` = linked, but Better Auth
 * could not produce a usable token (the refresh token expired or GitHub
 * revoked the grant) — the UI offers a reconnect, which re-links the same
 * account and rewrites the tokens.
 */
export async function githubUserToken(
  userId: string
): Promise<GithubUserTokenState> {
  const account = await githubAccountForUser(userId)
  if (!account) return { state: `none` }
  try {
    const tokens = await auth.api.getAccessToken({
      body: { providerId: GITHUB_PROVIDER_ID, userId },
    })
    const token = tokens?.accessToken
    if (!token) return { state: `dead` }
    return { state: `ok`, token }
  } catch {
    return { state: `dead` }
  }
}

/** The login of the linked GitHub account for labels (`acme-inc`), null when
 * unlinked or when GitHub refuses the token. */
export async function githubLoginForToken(token: string): Promise<string | null> {
  return (await getAuthenticatedGithubUser(token))?.login ?? null
}

/** The app user behind a GitHub numeric account id, through the `accounts`
 * rows Better Auth writes. Null for an unknown id — never a login fallback:
 * logins are renameable and re-registerable. */
export async function userIdForGithubAccountId(
  githubUserId: number
): Promise<string | null> {
  const [row] = await db
    .select({ userId: accounts.userId })
    .from(accounts)
    .where(
      and(
        eq(accounts.providerId, GITHUB_PROVIDER_ID),
        eq(accounts.accountId, String(githubUserId))
      )
    )
    .limit(1)
  return row?.userId ?? null
}
