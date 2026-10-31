// #VAPP-181: the app tool BROKER behind `/api/mcp/apps`, the ONE global
// endpoint of every app's exposed MCP tools (@exp/emcp exposes.json +
// access.json + upstream.json; the package format: eUI `exposes`, #VAPP-180).
// The gate (gate.ts) authenticates; this resolves the caller's `apps` grant
// (one team, all apps or a list, read | full), lists the head versions'
// `exposes` as `<app>__<tool>` filtered by grant ∩ member policy ∩ the tool's
// level, and runs a call as the CALLING member: a records query/write of the
// app's own kind, a granted non-destructive `exponential.<noun>.<op>`, or an
// upstream MCP tool with the member's own vaulted credential (a missing link
// answers a connect URL). Every result carries `_meta` provenance; nothing
// chains: one call runs exactly one binding of one app.
import { createHash } from "node:crypto"
import { Client } from "@modelcontextprotocol/sdk/client/index.js"
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js"
import { Server } from "@modelcontextprotocol/sdk/server/index.js"
import {
  CallToolRequestSchema,
  ErrorCode,
  ListToolsRequestSchema,
  McpError,
  type CallToolResult,
  type Tool,
} from "@modelcontextprotocol/sdk/types.js"
import { and, asc, desc, eq, isNull, sql } from "drizzle-orm"
import { actionGrantable, grantAllows, parseActionName } from "@exp/domain-contract/app-actions"
import { APP_RECORDS_QUERY } from "@exp/domain-contract/app-records"
import {
  effectiveAccess,
  grantErrors,
  toolVisible,
  type Grant,
  type GrantAccess,
  type GrantKind,
  type Level,
} from "@exp/emcp/access"
import type { CallOutcome } from "@exp/emcp/audit"
import {
  listToolNames,
  PROVENANCE_KEYS,
  provenanceMeta,
  resolveArgs,
  runVia,
  upstreamCallError,
  validateExposes,
  type ActionRun,
  type Expose,
  type RecordsRun,
  type UpstreamRun,
} from "@exp/emcp/exposes"
import { UPSTREAM_ERRORS } from "@exp/emcp/upstream"
import { db } from "@/db/connection"
import { appRecords, apps, mcpGrants, mcpServers, teamMembers, teams } from "@/db/schema"
import type { EmcpCaller, ServeOptions, ToolProvider } from "@/lib/emcp/gate"
import { resolveMcpToolGates } from "@/lib/emcp/gates"
import { jsonResponse } from "@/lib/emcp/helpers"
import { buildMcpAccess } from "@/lib/emcp/scope"
import { createExponentialMcpServer, type McpUser } from "@/lib/emcp/server"
import { caller as trpcCaller } from "@/lib/emcp/tools"
import { resolveForLaunch } from "@/lib/emcp/upstream/credentials"
import { callUpstreamTool } from "@/lib/emcp/upstream/mcp-client"
import { appBaseUrl } from "@/lib/notification-email-policy"

/** The endpoint's path (Settings › MCP shows it). */
export const APPS_MCP_PATH = `/api/mcp/apps`

/** The short error codes a broker answer starts with (`<code>: <words>`),
 * and how the audit reads each (@exp/emcp audit.json `callStatus`). */
export const BROKER_ERRORS: Record<string, CallOutcome> = {
  invalidArguments: { invalid: true },
  notGranted: { denied: true },
  accessTooLow: { denied: true },
  destructive: { denied: true },
  [UPSTREAM_ERRORS.missing]: { upstreamError: true },
  [UPSTREAM_ERRORS.disconnected]: { upstreamError: true },
  [UPSTREAM_ERRORS.failed]: { upstreamError: true },
  toolError: { error: true },
}

/** The `mcp_grants` row a request runs under, as @exp/emcp reads it. */
export async function appsGrantFor(userId: string, clientId: string): Promise<Grant | null> {
  const [row] = await db
    .select({
      kind: mcpGrants.kind,
      allTeams: mcpGrants.allTeams,
      teamIds: mcpGrants.teamIds,
      boardIds: mcpGrants.boardIds,
      allApps: mcpGrants.allApps,
      appIds: mcpGrants.appIds,
      access: mcpGrants.access,
    })
    .from(mcpGrants)
    .where(and(eq(mcpGrants.userId, userId), eq(mcpGrants.clientId, clientId), eq(mcpGrants.kind, `apps`)))
    .limit(1)
  return row ? { ...row, kind: row.kind as GrantKind, access: row.access as GrantAccess } : null
}

/** A first-party credential (cookie, session bearer, `expu_` key) names its
 * team with `?team=<id|slug>` and reaches every app of it at Full, within
 * what the credential itself reaches. */
async function firstPartyGrant(caller: EmcpCaller, request: Request): Promise<Grant | null> {
  const ref = new URL(request.url).searchParams.get(`team`)?.trim()
  if (!ref) return null
  const isUuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(ref)
  const [team] = await db
    .select({ id: teams.id })
    .from(teams)
    .where(isUuid ? eq(teams.id, ref) : eq(teams.slug, ref))
    .limit(1)
  if (!team) return null
  if (!caller.access.full && !caller.access.visibleTeamIds.has(team.id)) return null
  return { kind: `apps`, allTeams: false, teamIds: [team.id], boardIds: [], allApps: true, appIds: [], access: `full` }
}

/** The caller's apps grant, or null (a token without one, a first-party
 * credential without `?team=`). */
export async function resolveAppsGrant(caller: EmcpCaller, request: Request): Promise<Grant | null> {
  const grant = caller.clientId ? await appsGrantFor(caller.user.id, caller.clientId) : await firstPartyGrant(caller, request)
  return grant && grantErrors(grant).length === 0 ? grant : null
}

export interface BrokerApp {
  id: string
  slug: string
  name: string
  teamId: string
  grantedTools: string[]
  upstreams: string[]
  level: Level
}

/** One listed tool: its qualified name, its app and its binding. */
export interface BrokerTool {
  name: string
  app: BrokerApp
  expose: Expose
}

/** An action binding's operation, when the broker may list it: granted on
 * the app (VAPP-120), grantable, and never another app's (`apps` noun:
 * the broker never chains tools across apps). */
function actionListable(app: BrokerApp, action: string): boolean {
  const target = parseActionName(action)
  if (!target || target.tool === `exponential_apps`) return false
  return actionGrantable(action) && app.grantedTools.includes(action)
}

/** The tools a grant shows, in listing order (apps by slug). `policyOf` =
 * the member × app policy rows (Settings › MCP Access, #VAPP-183); none =
 * Full. */
export async function brokerTools(
  userId: string,
  grant: Grant,
  policyOf: (appId: string) => Level | null = () => null
): Promise<{ teamId: string; teamSlug: string; tools: BrokerTool[] }> {
  const teamId = grant.teamIds[0]!
  const [team] = await db.select({ slug: teams.slug }).from(teams).where(eq(teams.id, teamId)).limit(1)
  const [member] = await db
    .select({ role: teamMembers.role })
    .from(teamMembers)
    .where(and(eq(teamMembers.teamId, teamId), eq(teamMembers.userId, userId)))
    .limit(1)
  const rows = await db
    .select({ id: apps.id, slug: apps.slug, name: apps.name, teamId: apps.teamId, package: apps.package, grantedTools: apps.grantedTools })
    .from(apps)
    .where(and(eq(apps.teamId, teamId), isNull(apps.deletedAt)))
  const listed: { app: BrokerApp; exposes: Expose[] }[] = []
  for (const row of rows) {
    const pkg = (row.package ?? {}) as { exposes?: unknown; records?: Record<string, unknown>; upstreams?: unknown }
    if (!Array.isArray(pkg.exposes) || pkg.exposes.length === 0) continue
    const upstreams = Array.isArray(pkg.upstreams) ? pkg.upstreams.filter((u): u is string => typeof u === `string`) : []
    const exposes = pkg.exposes as Expose[]
    // Saved packages were validated; a binding that no longer is (a stale
    // row) is simply not offered.
    const broken = new Set(validateExposes(exposes, { kinds: Object.keys(pkg.records ?? {}), upstreams }).map((e) => e.index))
    if (broken.has(null)) continue
    const level = effectiveAccess({ grant, appId: row.id, teamId: row.teamId, appDeleted: false, role: member?.role ?? null, policy: policyOf(row.id) })
    listed.push({
      app: { id: row.id, slug: row.slug, name: row.name, teamId: row.teamId, grantedTools: row.grantedTools, upstreams, level },
      exposes: exposes.filter((_, i) => !broken.has(i)),
    })
  }
  // Names over every offered binding (stable whatever this caller sees),
  // then only what this caller may see.
  const bySlug = new Map(listed.map((l) => [l.app.slug, l]))
  const tools: BrokerTool[] = []
  for (const named of listToolNames(listed.map((l) => ({ slug: l.app.slug, exposes: l.exposes })))) {
    const entry = bySlug.get(named.app)!
    const expose = entry.exposes.find((e) => e.name === named.tool)!
    if (!toolVisible(entry.app.level, expose.access)) continue
    if (runVia(expose.run) === `action` && !actionListable(entry.app, (expose.run as ActionRun).action)) continue
    tools.push({ name: named.name, app: entry.app, expose })
  }
  return { teamId, teamSlug: team?.slug ?? ``, tools }
}

/** A listed tool as MCP's tools/list shows it: the version's frozen words,
 * the input schema, the read-only/destructive hints, the provenance. */
export function toolDefinition(tool: BrokerTool): Tool {
  const via = runVia(tool.expose.run)
  return {
    name: tool.name,
    title: tool.expose.title,
    description: tool.expose.description,
    inputSchema: tool.expose.input as Tool[`inputSchema`],
    annotations: {
      title: tool.expose.title,
      readOnlyHint: tool.expose.access === `read`,
      destructiveHint: false,
      openWorldHint: via === `upstream`,
    },
    _meta: provenanceMeta(tool.app.slug, tool.expose),
  }
}

/** A stable hash of what a caller is shown: a new head version, a trashed
 * app, a grant or policy change moves it (`notifications/tools/list_changed`). */
export function toolsFingerprint(tools: readonly BrokerTool[]): string {
  return createHash(`sha256`).update(JSON.stringify(tools.map(toolDefinition))).digest(`hex`)
}

const json = (value: unknown) => JSON.stringify(value)

function answer(tool: BrokerTool, value: unknown): CallToolResult {
  return {
    content: [{ type: `text`, text: json(value) }],
    structuredContent: (value && typeof value === `object` && !Array.isArray(value) ? value : { value }) as Record<string, unknown>,
    _meta: provenanceMeta(tool.app.slug, tool.expose),
  }
}

/** A broker refusal or failure: `<code>: <words>`, the provenance kept. */
export function brokerError(tool: BrokerTool, code: string, words: string): CallToolResult {
  return { isError: true, content: [{ type: `text`, text: `${code}: ${words}` }], _meta: provenanceMeta(tool.app.slug, tool.expose) }
}

function withProvenance(tool: BrokerTool, result: Record<string, unknown>): CallToolResult {
  const meta = result._meta && typeof result._meta === `object` ? (result._meta as Record<string, unknown>) : {}
  return { ...(result as CallToolResult), _meta: { ...meta, ...provenanceMeta(tool.app.slug, tool.expose) } }
}

/** The call's arguments against the tool's input schema: an object holding
 * every `required` key. Null = fine, else the words. */
export function argumentsError(expose: Expose, args: unknown): string | null {
  if (args === null || typeof args !== `object` || Array.isArray(args)) return `arguments are an object`
  const missing = (expose.input.required ?? []).filter((key) => !Object.prototype.hasOwnProperty.call(args, key))
  return missing.length > 0 ? `missing ${missing.join(`, `)}` : null
}

export interface BrokerContext {
  user: McpUser
  request: Request
  teamId: string
  teamSlug: string
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

async function runRecords(ctx: BrokerContext, tool: BrokerTool, input: Record<string, unknown>): Promise<CallToolResult> {
  const run = (tool.expose.run as RecordsRun).records
  const resolved = resolveArgs({ id: run.id, where: run.where, data: run.data, limit: run.limit }, input) as {
    id?: unknown
    where?: unknown
    data?: unknown
    limit?: unknown
  }
  const columns = { id: appRecords.id, data: appRecords.data, createdBy: appRecords.createdBy, createdAt: appRecords.createdAt, updatedAt: appRecords.updatedAt }
  const own = and(eq(appRecords.appId, tool.app.id), eq(appRecords.kind, run.kind), isNull(appRecords.deletedAt))
  if (run.op === `get`) {
    if (typeof resolved.id !== `string` || !UUID.test(resolved.id)) return brokerError(tool, `invalidArguments`, `a record id is a uuid`)
    const [row] = await db.select(columns).from(appRecords).where(and(own, eq(appRecords.id, resolved.id))).limit(1)
    return row ? answer(tool, { record: row }) : brokerError(tool, `toolError`, `record not found`)
  }
  if (run.op === `list`) {
    const where = resolved.where && typeof resolved.where === `object` && !Array.isArray(resolved.where) ? resolved.where : {}
    const asked = typeof resolved.limit === `number` && Number.isFinite(resolved.limit) ? Math.floor(resolved.limit) : APP_RECORDS_QUERY.limitDefault
    const limit = Math.min(Math.max(asked, 1), APP_RECORDS_QUERY.limitMax)
    const rows = await db
      .select(columns)
      .from(appRecords)
      .where(and(own, Object.keys(where).length > 0 ? sql`${appRecords.data} @> ${json(where)}::jsonb` : undefined))
      .orderBy(desc(appRecords.createdAt), asc(appRecords.id))
      .limit(limit)
    return answer(tool, { records: rows, count: rows.length })
  }
  // upsert: the records router's own validation, quota and patch merge.
  if (!resolved.data || typeof resolved.data !== `object` || Array.isArray(resolved.data)) return brokerError(tool, `invalidArguments`, `data is an object`)
  if (resolved.id !== undefined && (typeof resolved.id !== `string` || !UUID.test(resolved.id))) return brokerError(tool, `invalidArguments`, `a record id is a uuid`)
  try {
    const out = await trpcCaller(ctx.user, ctx.request).apps.records.upsert({
      id: tool.app.id,
      kind: run.kind,
      data: resolved.data as Record<string, unknown>,
      recordId: resolved.id as string | undefined,
    })
    return answer(tool, { id: out.id, created: out.created })
  } catch (e) {
    return brokerError(tool, `toolError`, e instanceof Error ? e.message.slice(0, 500) : `the write failed`)
  }
}

/** One of Exponential's own tools, run in-process as the member, confined to
 * the grant's team (the same handlers, checks and attribution as /api/mcp). */
async function callExponentialTool(ctx: BrokerContext, name: string, args: Record<string, unknown>): Promise<Record<string, unknown>> {
  const access = buildMcpAccess({ allTeams: false, teamIds: [ctx.teamId], boardIds: [] }, new Map())
  const gates = await resolveMcpToolGates(ctx.user.id, access, null)
  const server = createExponentialMcpServer(ctx.user, ctx.request, access, null, gates)
  const [clientSide, serverSide] = InMemoryTransport.createLinkedPair()
  const client = new Client({ name: `exponential-apps-broker`, version: `1.0.0` })
  try {
    await server.connect(serverSide)
    await client.connect(clientSide)
    return (await client.callTool({ name, arguments: args })) as Record<string, unknown>
  } finally {
    await client.close().catch(() => {})
    await server.close().catch(() => {})
  }
}

async function runAction(ctx: BrokerContext, tool: BrokerTool, input: Record<string, unknown>): Promise<CallToolResult> {
  const run = tool.expose.run as ActionRun
  const args = (resolveArgs(run.args ?? {}, input) ?? {}) as Record<string, unknown>
  if (!actionListable(tool.app, run.action) || !grantAllows(tool.app.grantedTools, run.action, args)) {
    return brokerError(tool, `notGranted`, `${run.action} is not granted on ${tool.app.name} (a team owner grants it on the app's settings page; a destructive call never runs)`)
  }
  const target = parseActionName(run.action)!
  try {
    const result = await callExponentialTool(ctx, target.tool, target.op ? { ...args, op: target.op } : args)
    return withProvenance(tool, result)
  } catch (e) {
    return brokerError(tool, `toolError`, e instanceof Error ? e.message.slice(0, 500) : `the operation failed`)
  }
}

function connectUrl(ctx: BrokerContext): string {
  return `${appBaseUrl()}/t/${ctx.teamSlug}/settings/mcp-servers`
}

async function runUpstream(ctx: BrokerContext, tool: BrokerTool, input: Record<string, unknown>): Promise<CallToolResult> {
  const run = tool.expose.run as UpstreamRun
  const args = (resolveArgs(run.args ?? {}, input) ?? {}) as Record<string, unknown>
  const [server] = tool.app.upstreams.includes(run.upstream)
    ? await db
        .select({ id: mcpServers.id, transport: mcpServers.transport })
        .from(mcpServers)
        .where(and(eq(mcpServers.teamId, ctx.teamId), eq(mcpServers.name, run.upstream)))
        .limit(1)
    : []
  if (!server) return brokerError(tool, UPSTREAM_ERRORS.missing, `the team has no MCP server ${run.upstream} (an owner adds it under Settings › MCP: ${connectUrl(ctx)})`)
  if (server.transport !== `http`) return brokerError(tool, UPSTREAM_ERRORS.failed, `${run.upstream} runs on a device (stdio); the server reaches only http servers`)
  const launch = await resolveForLaunch(db, { userId: ctx.user.id, teamIds: [ctx.teamId], serverIds: [server.id] })
  const entry = launch.servers[0]
  if (!entry?.url) {
    return brokerError(tool, UPSTREAM_ERRORS.disconnected, `connect your ${run.upstream} account first: ${connectUrl(ctx)}`)
  }
  const headers = Object.fromEntries(entry.headers.map((h) => [h.name, h.value]))
  const called = await callUpstreamTool(entry.url, headers, run.tool, args, (annotations) => upstreamCallError(tool.expose, annotations))
  if (called.ok) {
    const result = withProvenance(tool, called.result)
    return result
  }
  if (called.reason === `vetoed`) return brokerError(tool, called.error, `${run.upstream}'s ${run.tool} says it is ${called.error === `destructive` ? `destructive` : `not read-only`}: this tool does not run it`)
  if (called.reason === `refused`) return brokerError(tool, UPSTREAM_ERRORS.disconnected, `${run.upstream} refused your connection; reconnect: ${connectUrl(ctx)}`)
  return brokerError(tool, UPSTREAM_ERRORS.failed, `${run.upstream}: ${called.error}`)
}

/** Run one listed tool for the member. */
export async function runBrokerTool(ctx: BrokerContext, tool: BrokerTool, args: unknown): Promise<CallToolResult> {
  const invalid = argumentsError(tool.expose, args ?? {})
  if (invalid) return brokerError(tool, `invalidArguments`, invalid)
  const input = (args ?? {}) as Record<string, unknown>
  const via = runVia(tool.expose.run)
  if (via === `records`) return runRecords(ctx, tool, input)
  if (via === `action`) return runAction(ctx, tool, input)
  return runUpstream(ctx, tool, input)
}

const errorText = (result: { content?: unknown }): string => {
  const first = Array.isArray(result.content) ? (result.content.find((c) => c && typeof c === `object` && (c as { type?: unknown }).type === `text`) as { text?: unknown } | undefined) : undefined
  return typeof first?.text === `string` ? first.text : ``
}

/** An app call's audit outcome + code from its JSON-RPC response. */
export function classifyAppToolResponse(response: unknown): { outcome: CallOutcome; error: string | null } {
  if (!response || typeof response !== `object`) return { outcome: { error: true }, error: `noAnswer` }
  const r = response as { result?: unknown; error?: { code?: unknown; message?: unknown } }
  if (r.error) {
    const unknown = r.error.code === ErrorCode.InvalidParams && typeof r.error.message === `string` && /Tool \S+ not found/.test(r.error.message)
    return unknown ? { outcome: { denied: true }, error: `unknownTool` } : { outcome: { error: true }, error: `rpcError` }
  }
  const result = (r.result && typeof r.result === `object` ? r.result : {}) as { isError?: unknown; content?: unknown; _meta?: Record<string, unknown> }
  if (result.isError !== true) return { outcome: {}, error: null }
  const code = errorText(result).match(/^([A-Za-z]+): /)?.[1]
  if (code && BROKER_ERRORS[code]) return { outcome: BROKER_ERRORS[code]!, error: code }
  // An upstream's own error answer, passed through.
  if (result._meta?.[PROVENANCE_KEYS.via] === `upstream`) return { outcome: { upstreamError: true }, error: UPSTREAM_ERRORS.failed }
  return { outcome: { error: true }, error: `toolError` }
}

const noGrant = () =>
  jsonResponse(403, {
    jsonrpc: `2.0`,
    error: {
      code: -32000,
      message: `This connection has no apps grant: reconnect it and pick a team and its apps on the consent page (a first-party key names its team with ?team=<slug>).`,
    },
    id: null,
  })

/** The MCP server of one request: tools/list and tools/call over the
 * caller's broker tools. */
export function createAppsMcpServer(ctx: BrokerContext, tools: readonly BrokerTool[]): Server {
  const server = new Server(
    { name: `exponential-apps`, version: `0.1.0` },
    {
      capabilities: { tools: { listChanged: true } },
      instructions: `The tools of this team's Exponential apps, named <app>__<tool>. Each runs as you, inside its own app: an app's tool reads only that app's records and upstreams. Results carry _meta at.exponential/app + at.exponential/tool (whose words you read).`,
    }
  )
  const byName = new Map(tools.map((t) => [t.name, t]))
  server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: tools.map(toolDefinition) }))
  server.setRequestHandler(CallToolRequestSchema, async (req) => {
    const tool = byName.get(req.params.name)
    // A tool not listed for this caller answers MCP's unknown-tool error,
    // never a hint that it exists (access.json).
    if (!tool) throw new McpError(ErrorCode.InvalidParams, `Tool ${req.params.name} not found`)
    return runBrokerTool(ctx, tool, req.params.arguments)
  })
  return server
}

/** `/api/mcp/apps`'s tool provider: the grant, the tools, the audit context. */
export const appsToolProvider: ToolProvider = async (caller, request) => {
  const grant = await resolveAppsGrant(caller, request)
  if (!grant) return noGrant()
  const { teamId, teamSlug, tools } = await brokerTools(caller.user.id, grant)
  const appOf = new Map(tools.map((t) => [t.name, t.app.id]))
  const audit: ServeOptions = { teamId, appIdOf: (name) => appOf.get(name) ?? null, classify: classifyAppToolResponse }
  return { server: createAppsMcpServer({ user: caller.user, request, teamId, teamSlug }, tools), audit }
}

/** How often the GET stream re-reads what the caller is shown. */
export const LIST_CHANGED_POLL_MS = 15_000
/** A GET stream ends after this; the client reopens it. */
export const LIST_CHANGED_STREAM_MS = 10 * 60_000

/** The GET side of the Streamable HTTP transport, for one notification:
 * `notifications/tools/list_changed` whenever the caller's tools change
 * (a new head version, a trashed app, a grant change). Comments keep the
 * stream alive; it ends after `LIST_CHANGED_STREAM_MS`. */
export async function appsListChangedStream(caller: EmcpCaller, request: Request): Promise<Response> {
  if (!(request.headers.get(`accept`) ?? ``).includes(`text/event-stream`)) {
    return jsonResponse(406, { jsonrpc: `2.0`, error: { code: -32000, message: `GET opens an event stream: accept text/event-stream` }, id: null })
  }
  const grant = await resolveAppsGrant(caller, request)
  if (!grant) return noGrant()
  const shown = async () => toolsFingerprint((await brokerTools(caller.user.id, grant)).tools)
  let last = await shown()
  const encoder = new TextEncoder()
  let timer: ReturnType<typeof setInterval> | undefined
  let ender: ReturnType<typeof setTimeout> | undefined
  const stop = () => {
    clearInterval(timer)
    clearTimeout(ender)
  }
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      const send = (text: string) => {
        try {
          controller.enqueue(encoder.encode(text))
        } catch {
          stop()
        }
      }
      send(`: tools\n\n`)
      timer = setInterval(() => {
        void shown()
          .then((now) => {
            if (now !== last) {
              last = now
              send(`event: message\ndata: ${json({ jsonrpc: `2.0`, method: `notifications/tools/list_changed` })}\n\n`)
            } else {
              send(`: ok\n\n`)
            }
          })
          .catch(() => send(`: retry\n\n`))
      }, LIST_CHANGED_POLL_MS)
      ender = setTimeout(() => {
        stop()
        try {
          controller.close()
        } catch {
          // already closed
        }
      }, LIST_CHANGED_STREAM_MS)
      request.signal?.addEventListener(`abort`, stop)
    },
    cancel: stop,
  })
  return new Response(stream, { headers: { "content-type": `text/event-stream`, "cache-control": `no-cache`, connection: `keep-alive` } })
}
