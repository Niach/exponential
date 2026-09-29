// Per-MEMBER credentials for team MCP servers (`mcp_credentials`): the
// OAuth token set the hosted callback exchanged or the secret the member
// typed, encrypted at rest (crypto.ts). This module owns the three reads —
// a member's CONNECTION status per server (settings + launch pickers), the
// launch resolution (the caller's own values, refreshed server-side when an
// access token is about to expire) — and the writes. A row that no longer
// decrypts (BETTER_AUTH_SECRET rotated) reads as not connected, never as an
// error. Nothing here logs or returns a credential except resolveForLaunch
// to the credential's OWN member. Ciphertexts are AAD-bound to
// (server, member): a row copied onto another member does not decrypt.
import { and, eq, inArray, sql } from "drizzle-orm"
import { mcpCredentials, mcpServers, type McpCredential, type McpServer } from "@/db/schema"
import {
  credentialAad,
  decryptCredential,
  encryptCredential,
  type McpCredentialPayload,
} from "@/lib/mcp-oauth/crypto"
import {
  clientForRefresh,
  refreshTokens,
  type Db,
  type TokenSet,
} from "@/lib/mcp-oauth/oauth-client"
import { hostedCallbackUrl } from "@/lib/mcp-oauth/urls"

/** An access token expiring inside this window is refreshed at launch. */
export const MCP_REFRESH_MARGIN_MS = 10 * 60_000

export type McpConnectionStatus =
  | `not_needed`
  | `connected`
  | `not_connected`
  | `expired`
  | `error`

/** One member's connection to one server. none-auth: `not_needed`. oauth:
 * `connected` (usable or refreshable), `expired` (access expired, no
 * refresh token), `error` (the last refresh failed; `error` says why),
 * `not_connected`. secret: `connected` | `not_connected`. */
export interface McpConnection {
  status: McpConnectionStatus
  /** ISO — the OAuth access token's expiry. */
  expiresAt: string | null
  error: string | null
}

export function connectionFor(
  server: Pick<McpServer, `auth`>,
  row:
    | Pick<McpCredential, `serverId` | `userId` | `ciphertext` | `expiresAt` | `error`>
    | null
    | undefined,
  now = new Date()
): McpConnection {
  if (server.auth === `none`) {
    return { status: `not_needed`, expiresAt: null, error: null }
  }
  const payload = row
    ? decryptCredential(row.ciphertext, credentialAad(row.serverId, row.userId))
    : null
  const notConnected: McpConnection = { status: `not_connected`, expiresAt: null, error: null }
  if (!row || !payload) return notConnected
  if (server.auth === `secret`) {
    return payload.value ? { status: `connected`, expiresAt: null, error: null } : notConnected
  }
  if (!payload.accessToken) return notConnected
  const expiresAt = row.expiresAt?.toISOString() ?? null
  if (row.error) return { status: `error`, expiresAt, error: row.error }
  if (row.expiresAt && row.expiresAt.getTime() <= now.getTime() && !payload.refreshToken) {
    return { status: `expired`, expiresAt, error: null }
  }
  return { status: `connected`, expiresAt, error: null }
}

async function upsertCredential(
  db: Db,
  args: {
    serverId: string
    userId: string
    teamId: string
    payload: McpCredentialPayload
    expiresAt: Date | null
    issuer: string | null
    clientId: string | null
  },
  now = new Date()
): Promise<void> {
  const values = {
    ciphertext: encryptCredential(args.payload, credentialAad(args.serverId, args.userId)),
    expiresAt: args.expiresAt,
    issuer: args.issuer,
    clientId: args.clientId,
    error: null,
  }
  await db
    .insert(mcpCredentials)
    .values({ serverId: args.serverId, userId: args.userId, teamId: args.teamId, ...values })
    .onConflictDoUpdate({
      target: [mcpCredentials.serverId, mcpCredentials.userId],
      set: { ...values, teamId: args.teamId, updatedAt: now },
    })
}

function oauthPayload(tokens: TokenSet, tokenEndpoint: string): McpCredentialPayload {
  return {
    accessToken: tokens.accessToken,
    ...(tokens.refreshToken ? { refreshToken: tokens.refreshToken } : {}),
    tokenType: tokens.tokenType,
    tokenEndpoint,
  }
}

export async function storeOauthTokens(
  db: Db,
  args: {
    serverId: string
    userId: string
    teamId: string
    issuer: string
    clientId: string
    tokenEndpoint: string
    tokens: TokenSet
  },
  now = new Date()
): Promise<void> {
  await upsertCredential(
    db,
    {
      serverId: args.serverId,
      userId: args.userId,
      teamId: args.teamId,
      payload: oauthPayload(args.tokens, args.tokenEndpoint),
      expiresAt: args.tokens.expiresAt,
      issuer: args.issuer,
      clientId: args.clientId,
    },
    now
  )
}

export async function storeSecret(
  db: Db,
  args: { serverId: string; userId: string; teamId: string; value: string },
  now = new Date()
): Promise<void> {
  await upsertCredential(
    db,
    {
      serverId: args.serverId,
      userId: args.userId,
      teamId: args.teamId,
      payload: { value: args.value },
      expiresAt: null,
      issuer: null,
      clientId: null,
    },
    now
  )
}

export interface ResolvedMcpServer {
  id: string
  name: string
  transport: `http` | `stdio`
  url: string | null
  command: string | null
  args: string[]
  headers: Array<{ name: string; value: string }>
  env: Array<{ name: string; value: string }>
}

export interface ResolvedMcpLaunch {
  servers: ResolvedMcpServer[]
  /** Not connected / refresh failed / unknown id — the launch goes on. */
  skipped: Array<{ id: string; name: string; reason: string }>
  warnings: string[]
}

// Concurrent launches of one member must not both spend a rotating refresh
// token (the loser's would be revoked). Across processes/replicas: a
// transaction-scoped advisory lock per credential row, the row RE-READ under
// it (a launch that waited finds it already refreshed and takes that token)
// and the write a compare-and-swap on the ciphertext it refreshed from (a
// reconnect that landed meanwhile wins). Within one process: one in-flight
// refresh per row, shared.
const refreshing = new Map<string, Promise<RefreshOutcome>>()

type RefreshOutcome =
  | { ok: true; accessToken: string; tokenType: string | undefined }
  | { ok: false; error: string }

function isExpiring(expiresAt: Date | null, now: Date): boolean {
  return expiresAt !== null && expiresAt.getTime() - now.getTime() < MCP_REFRESH_MARGIN_MS
}

function refreshRow(
  db: Db,
  server: McpServer,
  rowId: string,
  now: Date
): Promise<RefreshOutcome> {
  const inflight = refreshing.get(rowId)
  if (inflight) return inflight
  const run = refreshLocked(db, server, rowId, now).finally(() => {
    refreshing.delete(rowId)
  })
  refreshing.set(rowId, run)
  return run
}

async function refreshLocked(
  db: Db,
  server: McpServer,
  rowId: string,
  now: Date
): Promise<RefreshOutcome> {
  return db.transaction(async (tx) => {
    await tx.execute(sql`select pg_advisory_xact_lock(hashtext(${rowId}))`)
    const [row] = await tx
      .select()
      .from(mcpCredentials)
      .where(eq(mcpCredentials.id, rowId))
      .limit(1)
    const aad = row ? credentialAad(row.serverId, row.userId) : ``
    const payload = row ? decryptCredential(row.ciphertext, aad) : null
    if (!row || !payload?.accessToken) {
      return { ok: false, error: `not connected` }
    }
    // Another launch refreshed while this one waited for the lock.
    if (!isExpiring(row.expiresAt, now)) {
      return { ok: true, accessToken: payload.accessToken, tokenType: payload.tokenType }
    }
    const unchanged = and(
      eq(mcpCredentials.id, row.id),
      eq(mcpCredentials.ciphertext, row.ciphertext)
    )
    try {
      if (!payload.tokenEndpoint || !row.clientId || !payload.refreshToken) {
        throw new Error(`no refresh token; reconnect`)
      }
      const client = await clientForRefresh(tx, row.issuer, row.clientId, hostedCallbackUrl())
      const tokens = await refreshTokens({
        tokenEndpoint: payload.tokenEndpoint,
        client,
        refreshToken: payload.refreshToken,
        resource: server.url!,
        now,
      })
      await tx
        .update(mcpCredentials)
        .set({
          ciphertext: encryptCredential(oauthPayload(tokens, payload.tokenEndpoint), aad),
          expiresAt: tokens.expiresAt,
          error: null,
          updatedAt: now,
        })
        .where(unchanged)
      return { ok: true, accessToken: tokens.accessToken, tokenType: tokens.tokenType }
    } catch (e) {
      const error = (e instanceof Error ? e.message : `refresh failed`).slice(0, 500)
      await tx.update(mcpCredentials).set({ error, updatedAt: now }).where(unchanged)
      return { ok: false, error }
    }
  })
}

function bearerHeader(tokenType: string | undefined, token: string): string {
  // Providers answer `bearer` in any case; the header wants the scheme.
  const scheme = !tokenType || tokenType.toLowerCase() === `bearer` ? `Bearer` : tokenType
  return `${scheme} ${token}`
}

/** The caller's OWN credentials for `serverIds`, limited to servers of
 * `teamIds` (the caller's teams). Never throws for one server: a server that
 * cannot be connected is SKIPPED with a reason. */
export async function resolveForLaunch(
  db: Db,
  args: { userId: string; teamIds: string[]; serverIds: string[] },
  now = new Date()
): Promise<ResolvedMcpLaunch> {
  const out: ResolvedMcpLaunch = { servers: [], skipped: [], warnings: [] }
  const ids = [...new Set(args.serverIds)]
  if (ids.length === 0) return out
  const servers =
    args.teamIds.length === 0
      ? []
      : await db
          .select()
          .from(mcpServers)
          .where(and(inArray(mcpServers.id, ids), inArray(mcpServers.teamId, args.teamIds)))
  const byId = new Map(servers.map((server) => [server.id, server]))
  const rows = servers.length === 0
    ? []
    : await db
        .select()
        .from(mcpCredentials)
        .where(
          and(
            inArray(mcpCredentials.serverId, servers.map((server) => server.id)),
            eq(mcpCredentials.userId, args.userId)
          )
        )
  const rowByServer = new Map(rows.map((row) => [row.serverId, row]))

  for (const id of ids) {
    const server = byId.get(id)
    if (!server) {
      out.skipped.push({ id, name: id, reason: `not found (removed, or not in your teams)` })
      continue
    }
    const entry: ResolvedMcpServer = {
      id: server.id,
      name: server.name,
      transport: server.transport === `stdio` ? `stdio` : `http`,
      url: server.url,
      command: server.command,
      args: server.args,
      headers: [],
      env: [],
    }
    if (server.auth === `none`) {
      out.servers.push(entry)
      continue
    }
    const row = rowByServer.get(server.id)
    const payload = row
      ? decryptCredential(row.ciphertext, credentialAad(row.serverId, row.userId))
      : null
    const skip = (reason: string) =>
      out.skipped.push({ id: server.id, name: server.name, reason })
    if (!row || !payload) {
      skip(`not connected`)
      continue
    }

    if (server.auth === `secret`) {
      const name = entry.transport === `http` ? server.headerNames[0] : server.envNames[0]
      if (!payload.value || !name) {
        skip(`not connected`)
        continue
      }
      if (entry.transport === `http`) entry.headers.push({ name, value: payload.value })
      else entry.env.push({ name, value: payload.value })
      out.servers.push(entry)
      continue
    }

    // oauth
    if (!payload.accessToken) {
      skip(`not connected`)
      continue
    }
    let accessToken = payload.accessToken
    let tokenType = payload.tokenType
    const expiresAt = row.expiresAt?.getTime() ?? null
    if (isExpiring(row.expiresAt, now)) {
      if (payload.refreshToken) {
        const refreshed = await refreshRow(db, server, row.id, now)
        if (!refreshed.ok) {
          skip(`sign-in refresh failed: ${refreshed.error}`)
          continue
        }
        accessToken = refreshed.accessToken
        tokenType = refreshed.tokenType
      } else if (expiresAt! <= now.getTime()) {
        skip(`sign-in expired; reconnect`)
        continue
      } else {
        const minutes = Math.max(1, Math.round((expiresAt! - now.getTime()) / 60_000))
        out.warnings.push(
          `${server.name}: access token expires in ${minutes} min and cannot be refreshed`
        )
      }
    }
    entry.headers.push({ name: `Authorization`, value: bearerHeader(tokenType, accessToken) })
    out.servers.push(entry)
  }
  return out
}
