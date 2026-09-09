// EXP-792: team MCP servers. The server holds NON-SECRET config only (names,
// url, header/env NAMES, scopes, the auth kind) plus a per-device readiness
// matrix; every credential lives in a device's 0600 secret store. Server-only
// (tRPC, never a shape — the natives have no decode). Member reads, owner
// writes; the device-side procedures (readiness, finishOAuth) authorize on
// the caller OWNING the device row. OAuth is web-initiated and
// device-executed: `beginOAuth` queues an `mcp_oauth_start` command, the
// device completes it EARLY with the authorize URL (devices.completeCommand
// → lib/mcp-oauth/flows.ts), the anonymous callback relays the code back as
// `mcp_oauth_code`, and the device's completion of THAT closes the flow.
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, inArray } from "drizzle-orm"
import {
  MAX_MCP_SERVER_NAME,
  MAX_MCP_SERVER_NAMES,
  mcpAuthSchema,
  mcpTransportSchema,
  mcpVariableNameSchema,
} from "@exp/db-schema/domain"
import { router, authedProcedure, type Context } from "@/lib/trpc"
import {
  deviceCommands,
  devices,
  mcpOauthFlows,
  mcpServerReadiness,
  mcpServers,
  type McpServer,
} from "@/db/schema"
import {
  assertTeamMember,
  assertTeamOwner,
  getUserTeamIds,
} from "@/lib/team-membership"
import { nudgeDevice, visibleDeviceRows } from "@/lib/trpc/devices"
import {
  applyReadinessReport,
  mcpReadinessEntriesSchema,
} from "@/lib/mcp-oauth/readiness"
import {
  defaultRedirect,
  expiresAtOrNull,
  finishFlow,
  flowIsExpired,
  flowIsPending,
  flowIsTerminal,
  MCP_OAUTH_FLOW_LIVE_STATUSES,
  newFlowState,
  redirectUriFor,
  upsertReadiness,
  type McpOauthRedirect,
} from "@/lib/mcp-oauth/flows"

const nameSchema = z.string().trim().min(1).max(MAX_MCP_SERVER_NAME)
const namesSchema = z.array(mcpVariableNameSchema).max(MAX_MCP_SERVER_NAMES)
const scopesSchema = z.array(z.string().trim().min(1).max(64)).max(16)
const argsSchema = z.array(z.string().max(1024)).max(64)
const deviceIdInput = z.string().min(1).max(128)

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
  envNames: namesSchema.optional(),
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

/** The caller's OWN device row for a steer deviceId (the device-side
 * procedures authorize on ownership, like the devices router). */
async function loadOwnDevice(
  db: Context[`db`],
  userId: string,
  deviceId: string
) {
  const [row] = await db
    .select({ id: devices.id, deviceId: devices.deviceId, caps: devices.caps })
    .from(devices)
    .where(and(eq(devices.userId, userId), eq(devices.deviceId, deviceId)))
    .limit(1)
  if (!row) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Device not found` })
  }
  return row
}

async function loadOwnFlow(db: Context[`db`], userId: string, flowId: string) {
  const [flow] = await db
    .select()
    .from(mcpOauthFlows)
    .where(and(eq(mcpOauthFlows.id, flowId), eq(mcpOauthFlows.userId, userId)))
    .limit(1)
  if (!flow) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Sign-in not found` })
  }
  return flow
}

function serializeFlow(
  flow: typeof mcpOauthFlows.$inferSelect,
  now = new Date()
) {
  // A flow the device never closed reads as failed once its TTL passed —
  // the web dialog polls this and must not spin forever.
  const expired = !flowIsTerminal(flow.status) && flowIsExpired(flow.createdAt, now)
  return {
    id: flow.id,
    serverId: flow.serverId,
    deviceRowId: flow.deviceRowId,
    redirect: flow.redirect as McpOauthRedirect,
    status: expired ? (`failed` as const) : flow.status,
    authorizeUrl: flow.authorizeUrl,
    error: expired ? `expired` : flow.error,
    createdAt: flow.createdAt,
    completedAt: flow.completedAt,
  }
}

export const mcpServersRouter = router({
  // One team's servers plus the readiness of every device the caller can
  // see (their own machines and the servers teammates shared with the
  // team) — the settings pane's "signed in on the MacBook, not on the mini".
  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await assertTeamMember(userId, input.teamId)
      const rows = await ctx.db
        .select()
        .from(mcpServers)
        .where(eq(mcpServers.teamId, input.teamId))
        .orderBy(asc(mcpServers.name))
      if (rows.length === 0) return []
      const visible = await visibleDeviceRows(ctx.db, userId, input.teamId)
      const deviceById = new Map(visible.rows.map((row) => [row.id, row]))
      const readiness =
        deviceById.size === 0
          ? []
          : await ctx.db
              .select()
              .from(mcpServerReadiness)
              .where(
                and(
                  inArray(
                    mcpServerReadiness.serverId,
                    rows.map((row) => row.id)
                  ),
                  inArray(mcpServerReadiness.deviceRowId, [
                    ...deviceById.keys(),
                  ])
                )
              )
      return rows.map((row) => ({
        ...row,
        readiness: readiness
          .filter((entry) => entry.serverId === row.id)
          .flatMap((entry) => {
            const device = deviceById.get(entry.deviceRowId)
            if (!device) return []
            return [
              {
                serverId: entry.serverId,
                deviceRowId: entry.deviceRowId,
                deviceId: device.deviceId,
                deviceLabel: device.label,
                userId: entry.userId,
                ready: entry.ready,
                expiresAt: entry.expiresAt?.toISOString() ?? null,
                error: entry.error,
                checkedAt: entry.checkedAt.toISOString(),
              },
            ]
          }),
      }))
    }),

  // Every server of every team the caller belongs to — the device resolves a
  // run's `mcpServerIds` against it and walks it on the readiness beat.
  listForDevice: authedProcedure.query(async ({ ctx }) => {
    const teamIds = await getUserTeamIds(ctx.session.user.id)
    if (teamIds.length === 0) return [] as McpServer[]
    return ctx.db
      .select()
      .from(mcpServers)
      .where(inArray(mcpServers.teamId, teamIds))
      .orderBy(asc(mcpServers.teamId), asc(mcpServers.name))
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
      const [row] = await ctx.db
        .update(mcpServers)
        .set({ ...values, updatedAt: new Date() })
        .where(eq(mcpServers.id, id))
        .returning()
      return row!
    }),

  // Readiness rows and flows cascade with the server.
  remove: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const current = await loadServer(ctx.db, input.id)
      await assertTeamOwner(ctx.session.user.id, current.teamId)
      await ctx.db.delete(mcpServers).where(eq(mcpServers.id, input.id))
      return { ok: true as const }
    }),

  // Start a sign-in on one of the caller's OWN machines. The flow row and
  // the `mcp_oauth_start` command land in one transaction; the device
  // completes the command early with the authorize URL (polled via
  // getOAuthFlow), the browser consents, and the code comes back through the
  // hosted callback or the device's loopback listener. A still-pending flow
  // for the same (server, device) is returned instead of a duplicate — the
  // device would otherwise run two PKCE dances for one sign-in.
  beginOAuth: authedProcedure
    .input(
      z.object({
        serverId: z.string().uuid(),
        deviceId: deviceIdInput,
        redirect: z.enum([`hosted`, `loopback`]).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const server = await loadServer(ctx.db, input.serverId)
      await assertTeamMember(userId, server.teamId)
      const device = await loadOwnDevice(ctx.db, userId, input.deviceId)
      if (server.auth !== `oauth`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `${server.name} does not use OAuth sign-in`,
        })
      }
      // The executor is a build that runs the mcp_oauth_* commands; a row
      // without the cap would leave the command pending forever.
      if (!(device.caps ?? []).includes(`mcp`)) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `That device runs a build without MCP server support. Update it first.`,
        })
      }
      const [existing] = await ctx.db
        .select()
        .from(mcpOauthFlows)
        .where(
          and(
            eq(mcpOauthFlows.serverId, server.id),
            eq(mcpOauthFlows.deviceRowId, device.id),
            inArray(mcpOauthFlows.status, [...MCP_OAUTH_FLOW_LIVE_STATUSES])
          )
        )
        .orderBy(desc(mcpOauthFlows.createdAt))
        .limit(1)
      if (existing && flowIsPending(existing)) {
        return {
          flowId: existing.id,
          state: existing.state,
          redirect: existing.redirect as McpOauthRedirect,
          existing: true as const,
        }
      }
      const redirect = input.redirect ?? defaultRedirect()
      const state = newFlowState()
      const flowId = await ctx.db.transaction(async (tx) => {
        const [flow] = await tx
          .insert(mcpOauthFlows)
          .values({
            state,
            userId,
            teamId: server.teamId,
            serverId: server.id,
            deviceRowId: device.id,
            redirect,
            status: `pending`,
          })
          .returning({ id: mcpOauthFlows.id })
        await tx.insert(deviceCommands).values({
          deviceRowId: device.id,
          userId,
          kind: `mcp_oauth_start`,
          payload: {
            serverId: server.id,
            state,
            redirectUri: redirectUriFor(redirect),
          },
        })
        return flow!.id
      })
      nudgeDevice(userId, device.deviceId)
      return { flowId, state, redirect, existing: false as const }
    }),

  getOAuthFlow: authedProcedure
    .input(z.object({ flowId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const flow = await loadOwnFlow(ctx.db, ctx.session.user.id, input.flowId)
      return serializeFlow(flow)
    }),

  cancelOAuth: authedProcedure
    .input(z.object({ flowId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const flow = await loadOwnFlow(ctx.db, ctx.session.user.id, input.flowId)
      await finishFlow(ctx.db, flow.id, { ok: false, error: `cancelled` })
      return { ok: true as const }
    }),

  // Device-side close of a flow, keyed on the state the device holds. The
  // loopback path has no code command to complete, so this is its only way
  // to report; the hosted path may call it too (idempotent: finishFlow moves
  // live rows only).
  finishOAuth: authedProcedure
    .input(
      z.object({
        state: z.string().min(1).max(64),
        ok: z.boolean(),
        expiresAt: z.string().max(64).optional(),
        error: z.string().max(500).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const [flow] = await ctx.db
        .select()
        .from(mcpOauthFlows)
        .where(eq(mcpOauthFlows.state, input.state))
        .limit(1)
      if (!flow) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Sign-in not found` })
      }
      const [device] = await ctx.db
        .select({ id: devices.id })
        .from(devices)
        .where(and(eq(devices.id, flow.deviceRowId), eq(devices.userId, userId)))
        .limit(1)
      if (!device) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the device that runs the sign-in can finish it`,
        })
      }
      const now = new Date()
      if (input.ok) {
        await finishFlow(ctx.db, flow.id, { ok: true }, now)
        await upsertReadiness(
          ctx.db,
          {
            serverId: flow.serverId,
            deviceRowId: flow.deviceRowId,
            userId: flow.userId,
            ready: true,
            expiresAt: expiresAtOrNull(input.expiresAt),
          },
          now
        )
      } else {
        await finishFlow(
          ctx.db,
          flow.id,
          { ok: false, error: input.error || `sign-in failed` },
          now
        )
      }
      return { ok: true as const }
    }),

  reportReadiness: authedProcedure
    .input(
      z.object({ deviceId: deviceIdInput, entries: mcpReadinessEntriesSchema })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const device = await loadOwnDevice(ctx.db, userId, input.deviceId)
      await applyReadinessReport(ctx.db, {
        userId,
        deviceRowId: device.id,
        entries: input.entries,
      })
      return { ok: true as const }
    }),
})
