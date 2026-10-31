// #VAPP-184: eMCP's revocation flows (@exp/emcp access.json "REVOCATION
// CASCADES", upstream.json `disconnect`):
// - a member revokes their own MCP client (an owner: anyone's in their
//   team): the client's grants for that member and its access + refresh
//   tokens go in one transaction, so the next request answers 401;
// - a member unlinks an upstream account: RFC 7009 revocation at the
//   provider (best effort), then the credential row goes;
// - a member leaving a team takes their apps grants for it, their access
//   policy rows and (when no grant is left) those clients' tokens with them
//   (their MCP credentials for the team's servers already go,
//   team-members.ts);
// - a trashed app drops its tools on the next tools/list (apps-broker.ts,
//   the GET stream's list_changed).
// A rotated VAULT_KEY re-seals lazily: the vault sweep (vault-sweep.ts).
import { and, eq, sql } from "drizzle-orm"
import { appAccess, mcpCredentials, mcpGrants, type McpCredential, type McpServer } from "@/db/schema"
import { oauthAccessTokens, oauthConsents } from "@/db/auth-schema"
import { decryptCredential } from "@/lib/emcp/upstream/crypto"
import { clientForRefresh, discover, revokeToken, type Db } from "@/lib/emcp/upstream/oauth-client"
import { hostedCallbackUrl } from "@/lib/emcp/upstream/urls"

type Tx = Parameters<Parameters<Db[`transaction`]>[0]>[0]
type Writer = Db | Tx

/** Revoke one member's MCP client: every grant (both kinds), the consent
 * and the tokens of that (member, client) pair. Returns how many grants
 * went. */
export async function revokeClient(db: Db, userId: string, clientId: string): Promise<number> {
  return db.transaction(async (tx) => {
    const gone = await tx
      .delete(mcpGrants)
      .where(and(eq(mcpGrants.userId, userId), eq(mcpGrants.clientId, clientId)))
      .returning({ id: mcpGrants.id })
    await tx.delete(oauthAccessTokens).where(and(eq(oauthAccessTokens.userId, userId), eq(oauthAccessTokens.clientId, clientId)))
    await tx.delete(oauthConsents).where(and(eq(oauthConsents.userId, userId), eq(oauthConsents.clientId, clientId)))
    return gone.length
  })
}

/** A member leaves `teamId`: their apps grants naming it, their access
 * policy rows in it, and the tokens of every client left without a grant.
 * Runs inside the removal's transaction. */
export async function revokeMemberMcpInTx(tx: Writer, teamId: string, userId: string): Promise<void> {
  const gone = await tx
    .delete(mcpGrants)
    .where(
      and(
        eq(mcpGrants.userId, userId),
        eq(mcpGrants.kind, `apps`),
        sql`${mcpGrants.teamIds} @> ${JSON.stringify([teamId])}::jsonb`
      )
    )
    .returning({ clientId: mcpGrants.clientId })
  for (const { clientId } of gone) {
    const [left] = await tx
      .select({ id: mcpGrants.id })
      .from(mcpGrants)
      .where(and(eq(mcpGrants.userId, userId), eq(mcpGrants.clientId, clientId)))
      .limit(1)
    if (!left) {
      await tx.delete(oauthAccessTokens).where(and(eq(oauthAccessTokens.userId, userId), eq(oauthAccessTokens.clientId, clientId)))
    }
  }
  await tx.delete(appAccess).where(and(eq(appAccess.teamId, teamId), eq(appAccess.userId, userId)))
}

/** RFC 7009 at the provider for one stored OAuth credential: the refresh
 * token (which takes its access tokens along), else the access token.
 * Best effort: false when nothing could be revoked. */
export async function revokeAtUpstream(db: Db, server: Pick<McpServer, `url` | `auth`>, row: McpCredential): Promise<boolean> {
  if (server.auth !== `oauth` || !server.url || !row.clientId) return false
  const payload = decryptCredential(row.ciphertext, row)
  const token = payload?.refreshToken ?? payload?.accessToken
  if (!token) return false
  try {
    const { revocationEndpoint } = await discover(server.url)
    if (!revocationEndpoint) return false
    const client = await clientForRefresh(db, row.issuer, row.clientId, hostedCallbackUrl())
    return await revokeToken({ revocationEndpoint, client, token, hint: payload?.refreshToken ? `refresh_token` : `access_token` })
  } catch {
    return false
  }
}

/** Unlink one member's upstream account: revoke at the provider, then
 * delete the credential (whatever the provider answered). */
export async function unlinkUpstream(db: Db, server: McpServer, userId: string): Promise<{ revoked: boolean }> {
  const [row] = await db
    .select()
    .from(mcpCredentials)
    .where(and(eq(mcpCredentials.serverId, server.id), eq(mcpCredentials.userId, userId)))
    .limit(1)
  const revoked = row ? await revokeAtUpstream(db, server, row) : false
  await db.delete(mcpCredentials).where(and(eq(mcpCredentials.serverId, server.id), eq(mcpCredentials.userId, userId)))
  return { revoked }
}
