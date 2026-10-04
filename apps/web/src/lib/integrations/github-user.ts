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

const GITHUB_TOKEN_ENDPOINT = `https://github.com/login/oauth/access_token`

/** The token shape Better Auth stores after a refresh (its `OAuth2Tokens`). */
export interface GithubRefreshedTokens {
  tokenType?: string
  accessToken: string
  refreshToken?: string
  accessTokenExpiresAt?: Date
  refreshTokenExpiresAt?: Date
  scopes?: string[]
}

/** Pure: GitHub's refresh answer as Better Auth's token shape. GitHub answers
 * a used or expired refresh token with HTTP 200 and `{"error": …}`; Better
 * Auth's stock refresh only throws on non-2xx, so it would store the OLD
 * refresh token back over a concurrent winner's new one. This throws instead,
 * which leaves the row untouched. */
export function githubTokensFromRefreshResponse(
  data: unknown,
  now: Date = new Date()
): GithubRefreshedTokens {
  const body = (data ?? {}) as Record<string, unknown>
  if (typeof body.error === `string` && body.error) {
    throw new Error(`GitHub refresh refused: ${body.error}`)
  }
  if (typeof body.access_token !== `string` || !body.access_token) {
    throw new Error(`GitHub refresh returned no access token`)
  }
  const at = (seconds: unknown) =>
    typeof seconds === `number` && seconds > 0
      ? new Date(now.getTime() + seconds * 1000)
      : undefined
  return {
    tokenType: typeof body.token_type === `string` ? body.token_type : undefined,
    accessToken: body.access_token,
    refreshToken:
      typeof body.refresh_token === `string` && body.refresh_token
        ? body.refresh_token
        : undefined,
    accessTokenExpiresAt: at(body.expires_in),
    refreshTokenExpiresAt: at(body.refresh_token_expires_in),
    scopes:
      typeof body.scope === `string` && body.scope
        ? body.scope.split(/[ ,]/).filter(Boolean)
        : undefined,
  }
}

/** The `github` social provider's `refreshAccessToken` (lib/auth/index.ts). */
export async function refreshGithubAccessToken(
  refreshToken: string,
  client: { clientId: string; clientSecret: string }
): Promise<GithubRefreshedTokens> {
  const response = await fetch(GITHUB_TOKEN_ENDPOINT, {
    method: `POST`,
    headers: {
      accept: `application/json`,
      "content-type": `application/x-www-form-urlencoded`,
    },
    body: new URLSearchParams({
      grant_type: `refresh_token`,
      client_id: client.clientId,
      client_secret: client.clientSecret,
      refresh_token: refreshToken,
    }),
  })
  if (!response.ok) {
    throw new Error(`GitHub refresh failed: HTTP ${response.status}`)
  }
  return githubTokensFromRefreshResponse(await response.json())
}

interface GithubProfile {
  id: number | string
  login?: string
  name?: string | null
  email?: string | null
  avatar_url?: string
}

interface GithubEmail {
  email: string
  primary?: boolean
  verified?: boolean
}

/** Pure: the sign-in identity for a GitHub profile. `github` is a trusted
 * provider (linking needs it), which skips Better Auth's emailVerified check
 * on implicit linking by email, so an UNVERIFIED address must never come
 * through: the primary verified address wins, else no email (sign-in then
 * fails with `email_not_found`; linking needs none). When the address list is
 * unreadable (`emails` null), the profile's public email stands: GitHub only
 * lets a verified address be public. */
export function githubUserInfo(
  profile: GithubProfile,
  emails: GithubEmail[] | null
): {
  id: string
  name: string
  email: string | null
  image?: string
  emailVerified: boolean
} {
  let email: string | null = null
  if (emails) {
    email = emails.find((e) => e.primary && e.verified)?.email ?? null
  } else if (profile.email) {
    email = profile.email
  }
  return {
    id: String(profile.id),
    name: profile.name || profile.login || ``,
    email,
    image: profile.avatar_url,
    emailVerified: email !== null,
  }
}

/** The `github` social provider's `getUserInfo` (lib/auth/index.ts). */
export async function fetchGithubUserInfo(token: { accessToken?: string }) {
  if (!token.accessToken) return null
  const headers = {
    accept: `application/vnd.github+json`,
    authorization: `Bearer ${token.accessToken}`,
    "user-agent": `exponential`,
  }
  const profileResponse = await fetch(`https://api.github.com/user`, { headers })
  if (!profileResponse.ok) return null
  const profile = (await profileResponse.json()) as GithubProfile
  const emailsResponse = await fetch(`https://api.github.com/user/emails`, {
    headers,
  }).catch(() => null)
  const emails =
    emailsResponse?.ok
      ? ((await emailsResponse.json().catch(() => null)) as GithubEmail[] | null)
      : null
  return {
    user: githubUserInfo(profile, Array.isArray(emails) ? emails : null),
    data: profile as unknown as Record<string, unknown>,
  }
}

// One lookup per user at a time. GitHub refresh tokens are single-use, and
// the integrations `status` and `repos` probes fire on the same focus event:
// two concurrent refreshes would race and the loser could kill the row. One
// web container, so in-process is enough.
const inflightTokens = new Map<string, Promise<GithubUserTokenState>>()

/**
 * The user's GitHub token, refreshed by Better Auth when the App expires user
 * tokens. `none` = no GitHub account linked; `dead` = linked, but Better Auth
 * could not produce a usable token (the refresh token expired or GitHub
 * revoked the grant) — the UI offers a reconnect, which re-links the same
 * account and rewrites the tokens. Concurrent callers share one lookup.
 */
export function githubUserToken(userId: string): Promise<GithubUserTokenState> {
  const pending = inflightTokens.get(userId)
  if (pending) return pending
  const lookup = resolveGithubUserToken(userId).finally(() => {
    inflightTokens.delete(userId)
  })
  inflightTokens.set(userId, lookup)
  return lookup
}

async function resolveGithubUserToken(
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
