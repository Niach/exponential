// EXP-792: team MCP servers — the ONE registry (no repo `.mcp.json`, no
// per-device servers). The row is NON-SECRET config (names, url, header/env
// NAMES, scopes, the auth kind); each MEMBER's credential (an OAuth token
// set or a typed secret) is held by the server, encrypted, in
// `mcp_credentials` (lib/mcp-oauth/credentials.ts), so one connect works on
// every device, remote start and automation. Server-only (tRPC, never a
// shape — the natives have no decode). Member reads + own-credential
// writes, owner registry writes. OAuth runs SERVER-side: `connect` returns
// the authorize URL, the anonymous hosted callback exchanges the code
// (lib/mcp-oauth/callback.ts). `resolveForLaunch` hands a launcher the
// caller's OWN values (on a shared device: the device owner's) for the
// servers its live run picked, named by `X-Exp-Session-Id` (EXP-1140); an
// unconnected or unpicked one is skipped with a reason, never a launch
// blocker.
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray } from "drizzle-orm"
import {
  MAX_MCP_SERVER_NAME,
  MAX_MCP_SERVER_NAMES,
  mcpAuthSchema,
  mcpEnvNameSchema,
  mcpTransportSchema,
  mcpVariableNameSchema,
} from "@exp/db-schema/domain"
import { router, authedProcedure, type Context } from "@/lib/trpc"
import {
  codingSessions,
  mcpCredentials,
  mcpOauthFlows,
  mcpServers,
  teamMembers,
  type McpServer,
} from "@/db/schema"
import { parseMcpSessionHeader } from "@/lib/mcp/session-header"
import { isAgentApiKeySession } from "@/lib/auth/api-key-kind"
import {
  assertTeamMember,
  assertTeamOwner,
  getUserTeamIds,
} from "@/lib/team-membership"
import {
  connectionFor,
  resolveForLaunch,
  storeSecret,
  type McpConnection,
} from "@/lib/mcp-oauth/credentials"
import { beginFlow } from "@/lib/mcp-oauth/flows"
import { safeReturnTo } from "@/lib/mcp-oauth/urls"
import { McpHttpError } from "@/lib/mcp-oauth/net"
import { McpOauthError } from "@/lib/mcp-oauth/oauth-client"
import { probeMcpServer, testMcpServer } from "@/lib/mcp-oauth/mcp-client"

export type { McpConnection, McpConnectionStatus } from "@/lib/mcp-oauth/credentials"
export type {
  ResolvedMcpLaunch,
  ResolvedMcpServer,
} from "@/lib/mcp-oauth/credentials"

/** One `mcpServers.list` row: the registry row plus the CALLER's connection
 * and how many current members connected. */
export type McpServerListRow = McpServer & {
  connection: McpConnection
  connectedCount: number
  memberCount: number
}

const nameSchema = z.string().trim().min(1).max(MAX_MCP_SERVER_NAME)
const namesSchema = z.array(mcpVariableNameSchema).max(MAX_MCP_SERVER_NAMES)
// Env names skip the launcher's own environment (PATH, NODE_*, CLAUDE_*, …).
const envNamesSchema = z.array(mcpEnvNameSchema).max(MAX_MCP_SERVER_NAMES)
const scopesSchema = z.array(z.string().trim().min(1).max(64)).max(16)
const argsSchema = z.array(z.string().max(1024)).max(64)

// http servers reach the public internet, so plain http is refused — except
// on the machine's own loopback (a local dev MCP), which never leaves it.
const urlSchema = z
  .string()
  .trim()
  .min(1)
  .max(2048)
  .refine(
    (value) => {
      let url: URL
      try {
        url = new URL(value)
      } catch {
        return false
      }
      if (url.protocol === `https:`) return true
      return (
        url.protocol === `http:` &&
        (url.hostname === `localhost` || url.hostname === `127.0.0.1`)
      )
    },
    { message: `The URL must be https:// (http:// only for localhost)` }
  )

const fieldsSchema = z.object({
  name: nameSchema,
  transport: mcpTransportSchema,
  url: urlSchema.optional(),
  headerNames: namesSchema.optional(),
  command: z.string().trim().min(1).max(1024).optional(),
  args: argsSchema.optional(),
  envNames: envNamesSchema.optional(),
  scopes: scopesSchema.optional(),
  auth: mcpAuthSchema,
  enabledByDefault: z.boolean().optional(),
})
type Fields = z.infer<typeof fieldsSchema>

/** The cross-field rules a row must satisfy (create and update alike, on the
 * MERGED row so a partial update cannot leave a stdio server with a url
 * and no command). Returns the normalized column set. */
function normalizeFields(fields: Fields): {
  name: string
  transport: Fields[`transport`]
  url: string | null
  headerNames: string[]
  command: string | null
  args: string[]
  envNames: string[]
  scopes: string[]
  auth: Fields[`auth`]
  enabledByDefault: boolean
} {
  const bad = (message: string): never => {
    throw new TRPCError({ code: `BAD_REQUEST`, message })
  }
  const http = fields.transport === `http`
  const headerNames = http ? dedupe(fields.headerNames ?? []) : []
  const envNames = http ? [] : dedupe(fields.envNames ?? [])
  if (http && !fields.url) bad(`An http server needs a URL`)
  if (!http && !fields.command) bad(`A stdio server needs a command`)
  if (fields.auth === `oauth` && !http) {
    bad(`OAuth sign-in applies to http servers only`)
  }
  if (fields.auth === `secret`) {
    const positions = http ? headerNames : envNames
    if (positions.length !== 1) {
      bad(
        http
          ? `A secret-authenticated http server declares exactly one header name (the secret's)`
          : `A secret-authenticated stdio server declares exactly one env name (the secret's)`
      )
    }
  }
  return {
    name: fields.name,
    transport: fields.transport,
    url: http ? fields.url! : null,
    headerNames,
    command: http ? null : fields.command!,
    args: http ? [] : (fields.args ?? []),
    envNames,
    scopes: dedupe(fields.scopes ?? []),
    auth: fields.auth,
    enabledByDefault: fields.enabledByDefault === true,
  }
}

function dedupe(values: string[]): string[] {
  return [...new Set(values)]
}

/** Whether an update re-targets where (or how) members' credentials are
 * sent: another url, transport, auth kind or header/env name. Members'
 * stored credentials and pending sign-ins for the old target are then void. */
export function retargetsCredentials(
  current: Pick<McpServer, `url` | `transport` | `auth` | `headerNames` | `envNames`>,
  next: Pick<McpServer, `url` | `transport` | `auth` | `headerNames` | `envNames`>
): boolean {
  const same = (a: string[], b: string[]) =>
    a.length === b.length && a.every((value, i) => value === b[i])
  return (
    current.url !== next.url ||
    current.transport !== next.transport ||
    current.auth !== next.auth ||
    !same(current.headerNames, next.headerNames) ||
    !same(current.envNames, next.envNames)
  )
}

async function loadServer(db: Context[`db`], id: string): Promise<McpServer> {
  const [row] = await db
    .select()
    .from(mcpServers)
    .where(eq(mcpServers.id, id))
    .limit(1)
  if (!row) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `MCP server not found` })
  }
  return row
}

/** The server a member-side procedure acts on: it exists and the caller is
 * a member of its team. */
async function loadMemberServer(
  ctx: Context & { session: { user: { id: string } } },
  id: string
): Promise<McpServer> {
  const server = await loadServer(ctx.db, id)
  await assertTeamMember(ctx.session.user.id, server.teamId)
  return server
}

/** An outbound failure (discovery, registration, the provider) as a tRPC
 * error with a sentence safe to show; anything else rethrows. */
function outboundError(e: unknown): never {
  if (e instanceof McpOauthError || e instanceof McpHttpError) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: e.message })
  }
  throw e
}

export const mcpServersRouter = router({
  // One team's servers with the CALLER's connection per server and the
  // team's connected/member counts (the settings pane + launch pickers).
  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }): Promise<McpServerListRow[]> => {
      const userId = ctx.session.user.id
      await assertTeamMember(userId, input.teamId)
      const rows = await ctx.db
        .select()
        .from(mcpServers)
        .where(eq(mcpServers.teamId, input.teamId))
        .orderBy(asc(mcpServers.name))
      if (rows.length === 0) return []
      const [credentials, members] = await Promise.all([
        ctx.db
          .select({
            serverId: mcpCredentials.serverId,
            userId: mcpCredentials.userId,
            ciphertext: mcpCredentials.ciphertext,
            expiresAt: mcpCredentials.expiresAt,
            error: mcpCredentials.error,
          })
          .from(mcpCredentials)
          .where(
            inArray(
              mcpCredentials.serverId,
              rows.map((row) => row.id)
            )
          ),
        ctx.db
          .select({ userId: teamMembers.userId })
          .from(teamMembers)
          .where(eq(teamMembers.teamId, input.teamId)),
      ])
      const memberIds = new Set(members.map((member) => member.userId))
      const now = new Date()
      return rows.map((row) => {
        const mine = credentials.find(
          (entry) => entry.serverId === row.id && entry.userId === userId
        )
        // A departed member's row lingers until they rejoin or the server
        // goes; it never counts.
        const connectedCount = credentials.filter(
          (entry) =>
            entry.serverId === row.id &&
            memberIds.has(entry.userId) &&
            connectionFor(row, entry, now).status === `connected`
        ).length
        return {
          ...row,
          connection: connectionFor(row, mine, now),
          // A none-auth server needs no connect: every member counts.
          connectedCount: row.auth === `none` ? memberIds.size : connectedCount,
          memberCount: memberIds.size,
        }
      })
    }),

  // What an http URL needs before an owner adds it: reachable? OAuth (and
  // which scopes it advertises) or none? A server-side fetch, so the
  // SSRF guard (lib/mcp-oauth/net.ts) applies.
  probe: authedProcedure
    .input(z.object({ teamId: z.string().uuid(), url: urlSchema }))
    .mutation(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)
      return probeMcpServer(input.url)
    }),

  create: authedProcedure
    .input(fieldsSchema.extend({ teamId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await assertTeamOwner(userId, input.teamId)
      const values = normalizeFields(input)
      const [dup] = await ctx.db
        .select({ id: mcpServers.id })
        .from(mcpServers)
        .where(
          and(eq(mcpServers.teamId, input.teamId), eq(mcpServers.name, values.name))
        )
        .limit(1)
      if (dup) {
        throw new TRPCError({
          code: `CONFLICT`,
          message: `A server named ${values.name} already exists in this team`,
        })
      }
      const [row] = await ctx.db
        .insert(mcpServers)
        .values({ ...values, teamId: input.teamId, createdById: userId })
        .returning()
      return row!
    }),

  update: authedProcedure
    .input(fieldsSchema.partial().extend({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const current = await loadServer(ctx.db, input.id)
      await assertTeamOwner(userId, current.teamId)
      const { id, ...patch } = input
      // Validate the MERGED row: a patch that only flips `auth` to secret
      // must still find exactly one declared name.
      const merged = fieldsSchema.parse({
        name: current.name,
        transport: current.transport,
        url: current.url ?? undefined,
        headerNames: current.headerNames,
        command: current.command ?? undefined,
        args: current.args,
        envNames: current.envNames,
        scopes: current.scopes,
        auth: current.auth,
        enabledByDefault: current.enabledByDefault,
        ...Object.fromEntries(
          Object.entries(patch).filter(([, value]) => value !== undefined)
        ),
      })
      const values = normalizeFields(merged)
      if (values.name !== current.name) {
        const [dup] = await ctx.db
          .select({ id: mcpServers.id })
          .from(mcpServers)
          .where(
            and(
              eq(mcpServers.teamId, current.teamId),
              eq(mcpServers.name, values.name)
            )
          )
          .limit(1)
        if (dup) {
          throw new TRPCError({
            code: `CONFLICT`,
            message: `A server named ${values.name} already exists in this team`,
          })
        }
      }
      // A re-targeted server drops every member's credential and pending
      // sign-in in the SAME transaction: a token minted for the old url (or
      // a secret typed for the old header/env) must never ride to the new
      // one. Members reconnect.
      const retarget = retargetsCredentials(current, values)
      const row = await ctx.db.transaction(async (tx) => {
        const [updated] = await tx
          .update(mcpServers)
          .set({ ...values, updatedAt: new Date() })
          .where(eq(mcpServers.id, id))
          .returning()
        if (retarget) {
          await tx.delete(mcpCredentials).where(eq(mcpCredentials.serverId, id))
          await tx
            .delete(mcpOauthFlows)
            .where(and(eq(mcpOauthFlows.serverId, id), eq(mcpOauthFlows.status, `pending`)))
        }
        return updated
      })
      return row!
    }),

  // Credentials and flows cascade with the server.
  remove: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const current = await loadServer(ctx.db, input.id)
      await assertTeamOwner(ctx.session.user.id, current.teamId)
      await ctx.db.delete(mcpServers).where(eq(mcpServers.id, input.id))
      return { ok: true as const }
    }),

  // Start the caller's OAuth sign-in: the browser opens `authorizeUrl`, the
  // hosted callback exchanges the code and stores the caller's tokens, then
  // 302s to `returnTo` (a same-origin RELATIVE path) with
  // `?mcp=connected|failed&server=<id>` — or renders a close-this-tab page.
  connect: authedProcedure
    .input(
      z.object({
        serverId: z.string().uuid(),
        returnTo: z.string().max(2048).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const server = await loadMemberServer(ctx, input.serverId)
      if (server.auth !== `oauth` || server.transport !== `http` || !server.url) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `${server.name} does not use OAuth sign-in`,
        })
      }
      let returnTo: string | null = null
      if (input.returnTo !== undefined) {
        returnTo = safeReturnTo(input.returnTo)
        if (!returnTo) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `returnTo must be a same-origin relative path`,
          })
        }
      }
      try {
        const flow = await beginFlow(ctx.db, {
          server,
          userId: ctx.session.user.id,
          returnTo,
        })
        return { authorizeUrl: flow.authorizeUrl }
      } catch (e) {
        outboundError(e)
      }
    }),

  // The caller's value for a `secret` server's ONE declared header/env name.
  setSecret: authedProcedure
    .input(
      z.object({
        serverId: z.string().uuid(),
        value: z
          .string()
          .min(1)
          .max(4096)
          .refine((value) => !value.includes(`\u0000`), `contains NUL bytes`),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const server = await loadMemberServer(ctx, input.serverId)
      if (server.auth !== `secret`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `${server.name} does not take a secret`,
        })
      }
      // A header value is one line; an env value may be anything but NUL.
      if (server.transport === `http` && /[\r\n]/.test(input.value)) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `A header secret must be a single line`,
        })
      }
      await storeSecret(ctx.db, {
        serverId: server.id,
        userId: ctx.session.user.id,
        teamId: server.teamId,
        value: input.value,
      })
      return { ok: true as const }
    }),

  // Forget the caller's credential (the provider-side grant stays; revoking
  // it there is the member's call).
  disconnect: authedProcedure
    .input(z.object({ serverId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const server = await loadMemberServer(ctx, input.serverId)
      await ctx.db
        .delete(mcpCredentials)
        .where(
          and(
            eq(mcpCredentials.serverId, server.id),
            eq(mcpCredentials.userId, ctx.session.user.id)
          )
        )
      return { ok: true as const }
    }),

  // MCP initialize + tools/list with the caller's credential (refreshed
  // like a launch would). http servers only: a stdio server runs on a
  // machine, never here.
  test: authedProcedure
    .input(z.object({ serverId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const server = await loadMemberServer(ctx, input.serverId)
      if (server.transport !== `http` || !server.url) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `Only http servers can be tested from here`,
        })
      }
      const resolved = await resolveForLaunch(ctx.db, {
        userId: ctx.session.user.id,
        teamIds: [server.teamId],
        serverIds: [server.id],
      })
      const entry = resolved.servers[0]
      if (!entry) {
        return {
          ok: false,
          tools: null,
          error: resolved.skipped[0]?.reason ?? `not connected`,
        }
      }
      return testMcpServer(
        server.url,
        Object.fromEntries(entry.headers.map((header) => [header.name, header.value]))
      )
    }),

  // The launcher's read (desktop, CLI daemon): the caller's OWN credentials
  // for the servers ONE live run picked. An OAuth token expiring within 10
  // minutes is refreshed and persisted first; a failed refresh is stored on
  // the row and the server SKIPPED.
  //
  // EXP-1140 — this is the one procedure that hands out decrypted secrets,
  // and the agent's `expu_` key reaches tRPC, so it is bound to a run:
  //   - `X-Exp-Session-Id` (the launcher's own request; the agent's MCP
  //     config carries the same header) names the `coding_sessions` row;
  //     missing → refused;
  //   - the row must be LIVE and the caller its owner or its host (a shared
  //     device's daemon calls as the device OWNER while the row belongs to
  //     the requester — the run then spends the owner's credentials, like
  //     its `expu_` key and GitHub token);
  //   - only the row's persisted pick (`mcp_server_ids`) resolves; anything
  //     else comes back `skipped` (never a blocker: a resume's recorded list
  //     may name a server removed since);
  //   - the agent's OWN key (`kind: agent`, lib/auth/api-key-kind.ts) is
  //     refused outright, so a prompt-injected agent cannot mint itself a
  //     row with a wider pick and read it back.
  resolveForLaunch: authedProcedure
    .input(z.object({ serverIds: z.array(z.string().uuid()).max(16) }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      if (await isAgentApiKeySession(ctx.db, ctx.session)) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `An agent's key never resolves MCP credentials — the launcher does, before the run starts.`,
        })
      }
      const sessionId = parseMcpSessionHeader(ctx.request)
      if (!sessionId) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `MCP credentials resolve only for a run the Exponential launcher started (missing X-Exp-Session-Id) — update Exponential on this device.`,
        })
      }
      const [run] = await ctx.db
        .select({
          id: codingSessions.id,
          userId: codingSessions.userId,
          hostUserId: codingSessions.hostUserId,
          teamId: codingSessions.teamId,
          status: codingSessions.status,
          mcpServerIds: codingSessions.mcpServerIds,
        })
        .from(codingSessions)
        .where(eq(codingSessions.id, sessionId))
        .limit(1)
      if (
        !run ||
        (run.userId !== userId && run.hostUserId !== userId) ||
        run.status === `ended`
      ) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `MCP credentials resolve only for a live run of your own.`,
        })
      }
      await assertTeamMember(userId, run.teamId)
      const picked = new Set(run.mcpServerIds ?? [])
      const serverIds = input.serverIds.filter((id) => picked.has(id))
      const resolved = await resolveForLaunch(ctx.db, {
        userId,
        teamIds: [run.teamId],
        serverIds,
      })
      for (const id of new Set(input.serverIds)) {
        if (!picked.has(id)) {
          resolved.skipped.push({ id, name: id, reason: `not picked for this run` })
        }
      }
      return resolved
    }),

  // Compat shim (cleanup round 29): desktop/CLI <= 0.14.56 resolve a start's
  // `mcp_server_ids` against this QUERY (no input; the per-device secret
  // store held the values) and turn a NOT_FOUND into a launch blocker that
  // refuses the start. Answer them with the plain registry rows of every
  // team the caller belongs to, in the old decoder's field set (non-secret
  // columns only; a credential never rides here). Delete once
  // CLIENT_MIN_VERSION_DESKTOP and _CLI pass 0.14.56.
  listForDevice: authedProcedure.query(async ({ ctx }) => {
    const teamIds = await getUserTeamIds(ctx.session.user.id)
    if (teamIds.length === 0) return []
    return ctx.db
      .select({
        id: mcpServers.id,
        teamId: mcpServers.teamId,
        name: mcpServers.name,
        transport: mcpServers.transport,
        url: mcpServers.url,
        headerNames: mcpServers.headerNames,
        command: mcpServers.command,
        args: mcpServers.args,
        envNames: mcpServers.envNames,
        scopes: mcpServers.scopes,
        auth: mcpServers.auth,
        enabledByDefault: mcpServers.enabledByDefault,
      })
      .from(mcpServers)
      .where(inArray(mcpServers.teamId, teamIds))
      .orderBy(asc(mcpServers.name))
  }),
})
