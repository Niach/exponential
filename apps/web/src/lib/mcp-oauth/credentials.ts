// Per-MEMBER credentials for team MCP servers (`mcp_credentials`): the
// OAuth token set the hosted callback exchanged or the secret the member
// typed, encrypted at rest (crypto.ts). This module owns the three reads —
// a member's CONNECTION status per server (settings + launch pickers), the
// launch resolution (the caller's own values, refreshed server-side when an
// access token is about to expire) — and the writes. A row that no longer
// decrypts (BETTER_AUTH_SECRET rotated) reads as not connected, never as an
// error. Nothing here logs or returns a credential except resolveForLaunch
// to the credential's OWN member — and (FEED-73) resolveSharedForLaunch to
// the launcher of an ACTION run in the same team, for the credentials their
// members marked `shared`. Ciphertexts are AAD-bound to (server, member): a
// row copied onto another member does not decrypt.
import { and, eq, inArray, sql } from "drizzle-orm"
import {
  mcpCredentials,
  mcpServers,
  teamMembers,
  users,
  type McpCredential,
  type McpServer,
} from "@/db/schema"
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
  /** FEED-73: the member shares this connection with the team's action
   * runs. False when there is no row. */
  shared: boolean
}

export function connectionFor(
  server: Pick<McpServer, `auth`>,
  row:
    | (Pick<McpCredential, `serverId` | `userId` | `ciphertext` | `expiresAt` | `error`> & {
        shared?: boolean | null
      })
    | null
    | undefined,
  now = new Date()
): McpConnection {
  const shared = row?.shared === true
  if (server.auth === `none`) {
    return { status: `not_needed`, expiresAt: null, error: null, shared: false }
  }
  const payload = row
    ? decryptCredential(row.ciphertext, credentialAad(row.serverId, row.userId))
    : null
  const notConnected: McpConnection = {
    status: `not_connected`,
    expiresAt: null,
    error: null,
    shared,
  }
  if (!row || !payload) return notConnected
  if (server.auth === `secret`) {
    return payload.value
      ? { status: `connected`, expiresAt: null, error: null, shared }
      : notConnected
  }
  if (!payload.accessToken) return notConnected
  const expiresAt = row.expiresAt?.toISOString() ?? null
  if (row.error) return { status: `error`, expiresAt, error: row.error, shared }
  if (row.expiresAt && row.expiresAt.getTime() <= now.getTime() && !payload.refreshToken) {
    return { status: `expired`, expiresAt, error: null, shared }
  }
  return { status: `connected`, expiresAt, error: null, shared }
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

/** FEED-73: whose credential an entry spends. `shared` = a teammate's
 * shared connection (the `<server>-as-<member>` entry), false = the caller's
 * own. NULL for a server that needs no credential. */
export interface McpActor {
  userId: string
  name: string
  shared: boolean
}

/** FEED-73: one current member × one picked credentialed server of an
 * ACTION run. `self` = the caller (the run spends their own entry),
 * `shared` = their shared connection resolved into an extra entry,
 * `unavailable` = shared but it failed to resolve (expired, refresh failed),
 * `connected` = connected but not shared (or a stdio server, which never
 * resolves for another member), `not_connected`. */
export interface McpMember {
  serverId: string
  serverName: string
  userId: string
  name: string
  state: `self` | `shared` | `connected` | `not_connected` | `unavailable`
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
  actor: McpActor | null
}

export interface ResolvedMcpLaunch {
  servers: ResolvedMcpServer[]
  /** Not connected / refresh failed / unknown id — the launch goes on. */
  skipped: Array<{ id: string; name: string; reason: string }>
  warnings: string[]
  /** FEED-73: the action run's member roster; [] for any other run. */
  members: McpMember[]
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

type EntryOutcome =
  | { entry: ResolvedMcpServer; warning?: string }
  | { skip: string }

/** One server's launch entry off ONE member's credential row (`row` unused
 * for none-auth). OAuth tokens expiring inside the margin are refreshed and
 * persisted first. `actor` is left NULL for the caller to stamp. */
async function resolveServerEntry(
  db: Db,
  server: McpServer,
  row: McpCredential | undefined,
  now: Date
): Promise<EntryOutcome> {
  const entry: ResolvedMcpServer = {
    id: server.id,
    name: server.name,
    transport: server.transport === `stdio` ? `stdio` : `http`,
    url: server.url,
    command: server.command,
    args: server.args,
    headers: [],
    env: [],
    actor: null,
  }
  if (server.auth === `none`) return { entry }
  const payload = row
    ? decryptCredential(row.ciphertext, credentialAad(row.serverId, row.userId))
    : null
  if (!row || !payload) return { skip: `not connected` }

  if (server.auth === `secret`) {
    const name = entry.transport === `http` ? server.headerNames[0] : server.envNames[0]
    if (!payload.value || !name) return { skip: `not connected` }
    if (entry.transport === `http`) entry.headers.push({ name, value: payload.value })
    else entry.env.push({ name, value: payload.value })
    return { entry }
  }

  // oauth
  if (!payload.accessToken) return { skip: `not connected` }
  let accessToken = payload.accessToken
  let tokenType = payload.tokenType
  let warning: string | undefined
  const expiresAt = row.expiresAt?.getTime() ?? null
  if (isExpiring(row.expiresAt, now)) {
    if (payload.refreshToken) {
      const refreshed = await refreshRow(db, server, row.id, now)
      if (!refreshed.ok) return { skip: `sign-in refresh failed: ${refreshed.error}` }
      accessToken = refreshed.accessToken
      tokenType = refreshed.tokenType
    } else if (expiresAt! <= now.getTime()) {
      return { skip: `sign-in expired; reconnect` }
    } else {
      const minutes = Math.max(1, Math.round((expiresAt! - now.getTime()) / 60_000))
      warning = `${server.name}: access token expires in ${minutes} min and cannot be refreshed`
    }
  }
  entry.headers.push({ name: `Authorization`, value: bearerHeader(tokenType, accessToken) })
  return { entry, warning }
}

/** The caller's OWN credentials for `serverIds`, limited to servers of
 * `teamIds` (the caller's teams). Never throws for one server: a server that
 * cannot be connected is SKIPPED with a reason. Credentialed entries carry
 * the caller as `actor` (`actorName` = their display name). */
export async function resolveForLaunch(
  db: Db,
  args: { userId: string; teamIds: string[]; serverIds: string[]; actorName?: string },
  now = new Date()
): Promise<ResolvedMcpLaunch> {
  const out: ResolvedMcpLaunch = { servers: [], skipped: [], warnings: [], members: [] }
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
    const outcome = await resolveServerEntry(db, server, rowByServer.get(server.id), now)
    if (`skip` in outcome) {
      out.skipped.push({ id: server.id, name: server.name, reason: outcome.skip })
      continue
    }
    if (server.auth !== `none`) {
      outcome.entry.actor = { userId: args.userId, name: args.actorName ?? ``, shared: false }
    }
    if (outcome.warning) out.warnings.push(outcome.warning)
    out.servers.push(outcome.entry)
  }
  return out
}

/** FEED-73: the longest launch entry name a shared connection gets. */
export const MCP_DELEGATE_NAME_MAX = 30
const MCP_DELEGATE_HANDLE_MAX = 12

function slug(value: string): string {
  return value
    .normalize(`NFKD`)
    .replace(/[\u0300-\u036f]/g, ``)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, `-`)
    .replace(/^-+|-+$/g, ``)
}

/** FEED-73: a member's handle in a shared entry's name (`linear-as-chris`):
 * the slug of their name's first word (else their email's local part, else
 * `member`), at most `max` (≤12) chars, uniqued against `taken` with `-2`,
 * `-3`… (added to `taken`). */
export function delegateHandle(
  name: string | null | undefined,
  email: string | null | undefined,
  taken: Set<string>,
  max = MCP_DELEGATE_HANDLE_MAX
): string {
  const limit = Math.max(1, Math.min(max, MCP_DELEGATE_HANDLE_MAX))
  const cut = (value: string, length: number) =>
    value.slice(0, Math.max(1, length)).replace(/-+$/, ``) || value.slice(0, 1)
  const first = slug((name ?? ``).trim().split(/\s+/)[0] ?? ``)
  const base = cut(first || slug((email ?? ``).split(`@`)[0] ?? ``) || `member`, limit)
  let handle = base
  for (let n = 2; taken.has(handle); n++) {
    const suffix = `-${n}`
    handle = `${cut(base, limit - suffix.length)}${suffix}`
  }
  taken.add(handle)
  return handle
}

/** FEED-73: the launch entry name of `serverName` spent as a member. */
export function delegateEntryName(
  serverName: string,
  name: string | null | undefined,
  email: string | null | undefined,
  taken: Set<string>
): string {
  const prefix = `${serverName}-as-`
  const room = MCP_DELEGATE_NAME_MAX - prefix.length
  return `${prefix}${delegateHandle(name, email, taken, room)}`
}

function displayName(user: { name: string | null; email: string | null } | undefined, userId: string) {
  return user?.name?.trim() || user?.email?.split(`@`)[0] || userId
}

export interface ResolvedMcpShares {
  servers: ResolvedMcpServer[]
  warnings: string[]
  /** serverId → members whose shared connection resolved into an entry. */
  resolved: Map<string, Set<string>>
  /** serverId → members who share but whose credential did not resolve
   * (never `skipped`: the run still has the caller's own entry). */
  unavailable: Map<string, Set<string>>
}

/** FEED-73: the extra entries an ACTION run gets — one per OTHER current
 * member of `teamId` who shared a connection to a picked HTTP server
 * (stdio servers run on the launching machine and never take another
 * member's secret). Each is named `<server>-as-<handle>` and carries the
 * sharer as `actor`. */
export async function resolveSharedForLaunch(
  db: Db,
  args: { teamId: string; serverIds: string[]; excludeUserId: string; now?: Date }
): Promise<ResolvedMcpShares> {
  const now = args.now ?? new Date()
  const out: ResolvedMcpShares = {
    servers: [],
    warnings: [],
    resolved: new Map(),
    unavailable: new Map(),
  }
  const ids = [...new Set(args.serverIds)]
  if (ids.length === 0) return out
  const servers = (
    await db
      .select()
      .from(mcpServers)
      .where(and(inArray(mcpServers.id, ids), eq(mcpServers.teamId, args.teamId)))
  ).filter((server) => server.transport === `http` && server.auth !== `none`)
  if (servers.length === 0) return out
  const [rows, members] = await Promise.all([
    db
      .select()
      .from(mcpCredentials)
      .where(
        and(
          inArray(mcpCredentials.serverId, servers.map((server) => server.id)),
          eq(mcpCredentials.shared, true)
        )
      ),
    db
      .select({ userId: teamMembers.userId })
      .from(teamMembers)
      .where(eq(teamMembers.teamId, args.teamId)),
  ])
  const memberIds = new Set(members.map((member) => member.userId))
  const shares = rows.filter(
    (row) =>
      row.shared === true &&
      row.userId !== args.excludeUserId &&
      memberIds.has(row.userId)
  )
  if (shares.length === 0) return out
  const people = await db
    .select({ id: users.id, name: users.name, email: users.email })
    .from(users)
    .where(inArray(users.id, [...new Set(shares.map((row) => row.userId))]))
  const personById = new Map(people.map((person) => [person.id, person]))
  const label = (userId: string) => displayName(personById.get(userId), userId)

  for (const id of ids) {
    const server = servers.find((candidate) => candidate.id === id)
    if (!server) continue
    const taken = new Set<string>()
    const mine = shares
      .filter((row) => row.serverId === server.id)
      .sort(
        (a, b) =>
          label(a.userId).localeCompare(label(b.userId)) || a.userId.localeCompare(b.userId)
      )
    for (const row of mine) {
      const outcome = await resolveServerEntry(db, server, row, now)
      if (`skip` in outcome) {
        const set = out.unavailable.get(server.id) ?? new Set<string>()
        set.add(row.userId)
        out.unavailable.set(server.id, set)
        continue
      }
      const person = personById.get(row.userId)
      outcome.entry.name = delegateEntryName(server.name, person?.name, person?.email, taken)
      outcome.entry.actor = { userId: row.userId, name: label(row.userId), shared: true }
      if (outcome.warning) out.warnings.push(`${outcome.warning} (${label(row.userId)})`)
      out.servers.push(outcome.entry)
      const set = out.resolved.get(server.id) ?? new Set<string>()
      set.add(row.userId)
      out.resolved.set(server.id, set)
    }
  }
  return out
}

/** FEED-73: every current member × every picked credentialed server of an
 * action run, with the state the launch left them in (McpMember). Sorted by
 * server name, then member name. */
export async function memberRoster(
  db: Db,
  args: {
    teamId: string
    serverIds: string[]
    selfUserId: string
    sharedResolved: Map<string, Set<string>>
    unavailable: Map<string, Set<string>>
    now?: Date
  }
): Promise<McpMember[]> {
  const now = args.now ?? new Date()
  const ids = [...new Set(args.serverIds)]
  if (ids.length === 0) return []
  const servers = (
    await db
      .select()
      .from(mcpServers)
      .where(and(inArray(mcpServers.id, ids), eq(mcpServers.teamId, args.teamId)))
  ).filter((server) => server.auth !== `none`)
  if (servers.length === 0) return []
  const [rows, members] = await Promise.all([
    db
      .select()
      .from(mcpCredentials)
      .where(inArray(mcpCredentials.serverId, servers.map((server) => server.id))),
    db
      .select({ userId: teamMembers.userId })
      .from(teamMembers)
      .where(eq(teamMembers.teamId, args.teamId)),
  ])
  const memberIds = [...new Set(members.map((member) => member.userId))]
  const people =
    memberIds.length === 0
      ? []
      : await db
          .select({ id: users.id, name: users.name, email: users.email })
          .from(users)
          .where(inArray(users.id, memberIds))
  const personById = new Map(people.map((person) => [person.id, person]))
  const out: McpMember[] = []
  for (const server of servers) {
    for (const userId of memberIds) {
      const row = rows.find((entry) => entry.serverId === server.id && entry.userId === userId)
      const state: McpMember[`state`] =
        userId === args.selfUserId
          ? `self`
          : args.sharedResolved.get(server.id)?.has(userId)
            ? `shared`
            : args.unavailable.get(server.id)?.has(userId)
              ? `unavailable`
              : connectionFor(server, row, now).status === `connected`
                ? `connected`
                : `not_connected`
      out.push({
        serverId: server.id,
        serverName: server.name,
        userId,
        name: displayName(personById.get(userId), userId),
        state,
      })
    }
  }
  return out.sort(
    (a, b) =>
      a.serverName.localeCompare(b.serverName) ||
      a.name.localeCompare(b.name) ||
      a.userId.localeCompare(b.userId)
  )
}
