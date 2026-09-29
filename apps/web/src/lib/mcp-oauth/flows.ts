// EXP-792: one member's MCP OAuth sign-in, run by the SERVER. `beginFlow`
// (mcpServers.connect) discovers the provider, picks the client, mints
// `state` + a PKCE pair (the verifier stored ENCRYPTED on the flow row) and
// returns the authorize URL; the anonymous hosted callback (callback.ts)
// matches the state, `completeFlow` exchanges the code and upserts the
// member's `mcp_credentials` row. Shared by the router and the callback, so
// this module imports NO router.
import { randomBytes } from "node:crypto"
import { and, eq, lt } from "drizzle-orm"
import { mcpOauthFlows, type McpOauthFlow, type McpServer } from "@/db/schema"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { hostedCallbackUrl } from "@/lib/mcp-oauth/urls"
import { decryptSecret, encryptSecret, flowVerifierAad } from "@/lib/mcp-oauth/crypto"
import {
  assertEndpointUrl,
  authorizeUrl,
  chooseClient,
  clientForRefresh,
  discover,
  exchangeCode,
  generatePkce,
  scopeFor,
  type Db,
} from "@/lib/mcp-oauth/oauth-client"
import { storeOauthTokens } from "@/lib/mcp-oauth/credentials"

export type { Db }
export {
  hostedCallbackUrl,
  isPublicHttpsBase,
  returnUrl,
  safeReturnTo,
} from "@/lib/mcp-oauth/urls"

/** A flow not finished within this window is refused by the callback. */
export const MCP_OAUTH_FLOW_TTL_MS = 10 * 60_000

export const MCP_OAUTH_FLOW_STATUSES = [`pending`, `done`, `failed`] as const
export type McpOauthFlowStatus = (typeof MCP_OAUTH_FLOW_STATUSES)[number]

/** 32 random bytes, base64url — the one value the callback keys on. */
export function newFlowState(): string {
  return randomBytes(32).toString(`base64url`)
}

export function flowIsExpired(createdAt: Date, now = new Date()): boolean {
  return now.getTime() - createdAt.getTime() >= MCP_OAUTH_FLOW_TTL_MS
}

/** The callback accepts a state only once, while pending, inside the TTL. */
export function flowAcceptsCode(
  flow: { status: string; createdAt: Date },
  now = new Date()
): boolean {
  return flow.status === `pending` && !flowIsExpired(flow.createdAt, now)
}

/** Discover + choose the client + mint the flow row; returns the authorize
 * URL the member's browser opens. Errors throw McpOauthError / McpHttpError
 * with a sentence safe to show. */
export async function beginFlow(
  db: Db,
  args: {
    server: McpServer
    userId: string
    returnTo: string | null
    appBase?: string
  }
): Promise<{ authorizeUrl: string; flowId: string }> {
  const base = args.appBase ?? appBaseUrl()
  const resource = args.server.url!
  const redirectUri = hostedCallbackUrl(base)
  const discovery = await discover(resource)
  // discover() already refuses non-https endpoints; re-check the one the
  // member's browser will navigate to before anything is stored.
  assertEndpointUrl(discovery.authorizationEndpoint, resource, `authorization endpoint`)
  const client = await chooseClient(db, discovery, base, redirectUri)
  const pkce = generatePkce()
  const state = newFlowState()
  // The caller's flows past the TTL are dead either way; drop them here so
  // the table holds only live sign-ins (no sweep needed).
  await db
    .delete(mcpOauthFlows)
    .where(
      and(
        eq(mcpOauthFlows.userId, args.userId),
        lt(mcpOauthFlows.createdAt, new Date(Date.now() - MCP_OAUTH_FLOW_TTL_MS))
      )
    )
  const url = authorizeUrl({
    authorizationEndpoint: discovery.authorizationEndpoint,
    clientId: client.clientId,
    redirectUri,
    codeChallenge: pkce.challenge,
    state,
    scope: scopeFor(args.server.scopes, discovery),
    resource,
  })
  const [flow] = await db
    .insert(mcpOauthFlows)
    .values({
      state,
      userId: args.userId,
      teamId: args.server.teamId,
      serverId: args.server.id,
      codeVerifierCiphertext: encryptSecret(pkce.verifier, flowVerifierAad(state)),
      clientId: client.clientId,
      issuer: discovery.issuer,
      tokenEndpoint: discovery.tokenEndpoint,
      resource,
      redirectUri,
      returnTo: args.returnTo,
      status: `pending`,
    })
    .returning({ id: mcpOauthFlows.id })
  return { flowId: flow!.id, authorizeUrl: url }
}

/** Claim a pending flow for this callback: the ONE transition out of
 * `pending` (a redelivered redirect finds it gone). Returns false when
 * another request won. */
export async function claimFlow(
  db: Db,
  flowId: string,
  outcome: { ok: true } | { ok: false; error: string },
  now = new Date()
): Promise<boolean> {
  const moved = await db
    .update(mcpOauthFlows)
    .set({
      status: outcome.ok ? `done` : `failed`,
      error: outcome.ok ? null : outcome.error.slice(0, 500),
      completedAt: now,
    })
    .where(and(eq(mcpOauthFlows.id, flowId), eq(mcpOauthFlows.status, `pending`)))
    .returning({ id: mcpOauthFlows.id })
  return moved.length > 0
}

/** Exchange `code` for the claimed flow and store the member's tokens.
 * Throws McpOauthError on a refused exchange. */
export async function completeFlow(
  db: Db,
  flow: McpOauthFlow,
  code: string,
  now = new Date()
): Promise<void> {
  const verifier = decryptSecret(flow.codeVerifierCiphertext, flowVerifierAad(flow.state))
  if (!verifier) throw new Error(`the sign-in expired`)
  const client = await clientForRefresh(db, flow.issuer, flow.clientId, flow.redirectUri)
  const tokens = await exchangeCode({
    tokenEndpoint: flow.tokenEndpoint,
    client,
    code,
    redirectUri: flow.redirectUri,
    codeVerifier: verifier,
    resource: flow.resource,
    now,
  })
  await storeOauthTokens(
    db,
    {
      serverId: flow.serverId,
      userId: flow.userId,
      teamId: flow.teamId,
      issuer: flow.issuer,
      clientId: flow.clientId,
      tokenEndpoint: flow.tokenEndpoint,
      tokens,
    },
    now
  )
}
