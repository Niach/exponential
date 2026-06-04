import { randomBytes, randomUUID } from "node:crypto"
import { db } from "@/db/connection"
import { oauthAccessTokens, oauthApplications } from "@/db/auth-schema"

// Keep these aligned with the `mcp()` plugin's accessTokenExpiresIn /
// refreshTokenExpiresIn (apps/web/src/lib/auth/index.ts) so the initial pair
// matches what a subsequent /mcp/token refresh issues.
const ACCESS_TOKEN_TTL_MS = 60 * 60 * 24 * 1000 // 1 day
const REFRESH_TOKEN_TTL_MS = 60 * 60 * 24 * 90 * 1000 // 90 days

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

function newToken(): string {
  return randomBytes(32).toString(`hex`)
}

export interface AgentCredential {
  accessToken: string
  refreshToken: string
  accessTokenExpiresAt: Date
  clientId: string
  tokenEndpoint: string
}

// Mints a refreshable OAuth credential for an agent sub-identity. It creates a
// per-agent **public** OAuth client (oauth_applications) + an access/refresh
// token pair (oauth_access_tokens) scoped to the agent user. `getMcpSession`
// resolves the access token to the agent user (plain DB lookup), and the agent
// refreshes via `POST {tokenEndpoint}` `grant_type=refresh_token&client_id=...`
// — a public client needs no secret. This mirrors exactly what better-auth's
// /mcp/token refresh grant issues, so the row is a first-class OAuth token.
export async function mintAgentCredential(
  tx: Tx,
  args: { agentUserId: string; agentName: string; baseUrl: string }
): Promise<AgentCredential> {
  const clientId = `expa_${randomBytes(18).toString(`base64url`)}`
  const accessToken = newToken()
  const refreshToken = newToken()
  const now = new Date()
  const accessTokenExpiresAt = new Date(now.getTime() + ACCESS_TOKEN_TTL_MS)
  const refreshTokenExpiresAt = new Date(now.getTime() + REFRESH_TOKEN_TTL_MS)

  await tx.insert(oauthApplications).values({
    id: randomUUID(),
    name: `Agent: ${args.agentName}`,
    clientId,
    // Public client (native app): no secret, so refresh needs only client_id +
    // refresh_token. redirectUrls is required-but-unused (we never run the
    // browser authorize flow; tokens are minted server-side here).
    clientSecret: null,
    redirectUrls: `${args.baseUrl}/api/auth/mcp/callback`,
    type: `public`,
    disabled: false,
    userId: args.agentUserId,
    createdAt: now,
    updatedAt: now,
  })

  await tx.insert(oauthAccessTokens).values({
    id: randomUUID(),
    accessToken,
    refreshToken,
    accessTokenExpiresAt,
    refreshTokenExpiresAt,
    clientId,
    userId: args.agentUserId,
    scopes: `agent`,
    createdAt: now,
    updatedAt: now,
  })

  return {
    accessToken,
    refreshToken,
    accessTokenExpiresAt,
    clientId,
    tokenEndpoint: `${args.baseUrl}/api/auth/mcp/token`,
  }
}
