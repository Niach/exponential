import { z } from "zod"
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { registerExponentialTools } from "./tools"
import { ALWAYS_LOAD_META } from "./always-load"
import { ALL_MCP_TOOL_GATES, type McpToolGates } from "./gates"
import type { McpUser } from "./server"
import type { McpAccess } from "./scope"
import { err } from "./helpers"

// NEW-4 (plan 3.11): the MCP surface is 15 tools, the SAME set for every
// caller, no conditional registration. ONE tool per noun takes an `op` and
// that op's parameters flat beside it (`{op: "get", id: "EXP-42"}`); two
// verbs stay whole (`sessions_send`, `guide_write`). The 79 per-operation
// tools of EXP-988 become the ops: `registerExponentialTools` still defines
// every one of them (their zod schemas, access checks and handlers are
// untouched), a recorder captures them instead of the server, and each noun
// routes `op` to the recorded tool: the op's STRICT schema validates the
// flat arguments (an unknown key is still an error), then its handler runs.
// An op the recorder never saw for this caller (a self-only op outside a
// session, the cloud-only bug report) errors with a message, never a
// missing tool. The description of a noun lists every op with its
// parameters, generated off the schemas so it can never drift; the JSON
// schema itself carries only `op` (the per-tool budget, context-budget.test.ts).
//
// Display ×4: a call's row in `contract.expToolDisplay.tools` is
// `<noun>_<op>` (`issues_get`, `people_invites_create`), composed by the
// engine from the call's name and its `op` (`crates/engine/src/mapper.rs`),
// so every client's lookup and copy stay keyed by that row name.

type ToolResult = { content: unknown[]; isError?: boolean; structuredContent?: unknown }
type LegacyHandler = (args: Record<string, unknown>, extra: unknown) => Promise<ToolResult>

export interface LegacyToolDef {
  description?: string
  inputSchema?: z.ZodType
  annotations?: Record<string, unknown>
  _meta?: Record<string, unknown>
}

export interface LegacyTool {
  name: string
  def: LegacyToolDef
  handler: LegacyHandler
}

export interface NounSpec {
  /** The wire name without the prefix (`issues`). */
  name: string
  /** The first sentence of the description. */
  blurb: string
  /** `op` → the recorded (EXP-988) tool it routes to; absent for a verb. */
  ops?: Record<string, string>
  /** A verb tool registered whole under this name. */
  verb?: string
  alwaysLoad?: boolean
}

const PREFIX = `exponential_`

/** The 15. Order = the order the tools list. */
export const NOUN_TOOLS: readonly NounSpec[] = [
  {
    name: `issues`,
    blurb: `Issues (id = UUID or identifier like EXP-12; list = open only unless includeClosed or a status filter).`,
    alwaysLoad: true,
    ops: {
      list: `issues_list`,
      get: `issues_get`,
      show: `issues_show`,
      create: `issues_create`,
      update: `issues_update`,
      delete: `issues_delete`,
      activity: `issues_activity`,
      update_status: `issues_update_status`,
      subscribe: `issues_subscribe`,
      unsubscribe: `issues_unsubscribe`,
      labels_add: `issue_labels_add`,
      labels_remove: `issue_labels_remove`,
      relations_add: `issue_relations_add`,
      relations_remove: `issue_relations_remove`,
    },
  },
  {
    name: `comments`,
    blurb: `Comments on an issue (plain GFM; threads one level deep; audience reporter emails a widget reporter).`,
    alwaysLoad: true,
    ops: { list: `comments_list`, create: `comments_create`, update: `comments_update`, delete: `comments_delete` },
  },
  {
    name: `attachments`,
    blurb: `Files on an issue: list, get (metadata + contents or a signed URL), upload (bytes now, or a signed uploadUrl then finalize), delete.`,
    ops: {
      list: `attachments_list`,
      get: `attachments_get`,
      upload: `attachments_upload`,
      delete: `attachments_delete`,
    },
  },
  {
    name: `boards`,
    blurb: `Boards of a team: list, get, create, update, delete (trash), set_repository.`,
    ops: {
      list: `boards_list`,
      get: `boards_get`,
      create: `boards_create`,
      update: `boards_update`,
      delete: `boards_delete`,
      set_repository: `boards_set_repository`,
    },
  },
  {
    name: `teams`,
    blurb: `Teams you are a member of: list, get, create, update.`,
    ops: { list: `teams_list`, get: `teams_get`, create: `teams_create`, update: `teams_update` },
  },
  {
    name: `people`,
    blurb: `A team's members and invites: list/add members, invites_list/invites_create/invites_revoke.`,
    ops: {
      list: `members_list`,
      add: `members_add`,
      invites_list: `invites_list`,
      invites_create: `invites_create`,
      invites_revoke: `invites_revoke`,
    },
  },
  {
    name: `team_config`,
    blurb: `A team's configuration: labels, issue statuses, saved prompts, agents, MCP servers and repositories (each noun_op).`,
    ops: {
      labels_list: `labels_list`,
      labels_get: `labels_get`,
      labels_create: `labels_create`,
      labels_update: `labels_update`,
      labels_delete: `labels_delete`,
      statuses_list: `statuses_list`,
      statuses_create: `statuses_create`,
      statuses_update: `statuses_update`,
      statuses_delete: `statuses_delete`,
      prompts_list: `prompts_list`,
      prompts_get: `prompts_get`,
      agents_list: `agents_list`,
      mcp_servers_list: `mcp_servers_list`,
      mcp_servers_add: `mcp_servers_add`,
      mcp_servers_remove: `mcp_servers_remove`,
      repositories_list: `repositories_list`,
      repositories_add: `repositories_add`,
    },
  },
  { name: `sessions_send`, blurb: ``, verb: `sessions_send`, alwaysLoad: true },
  {
    name: `sessions`,
    blurb: `Sessions (threads, agents, children): get a tree (no id = yours), list, link/unlink issues, end one.`,
    ops: {
      get: `sessions_get`,
      list: `sessions_list`,
      link: `sessions_link`,
      unlink: `sessions_unlink`,
      end: `sessions_end`,
    },
  },
  {
    name: `session_self`,
    blurb: `Your own session: ask a person one question (card), plan (plan mode's card), suggest replies or tasks, compact your context, report_bug when Exponential itself misbehaves.`,
    ops: {
      ask: `sessions_ask`,
      plan: `plan_propose`,
      suggest: `sessions_suggest`,
      compact: `sessions_compact`,
      report_bug: `report_bug`,
    },
  },
  { name: `guide_write`, blurb: ``, verb: `guide_write`, alwaysLoad: true },
  {
    name: `pr`,
    blurb: `Pull requests (name one by pullRequestId, issueId or repositoryId + prNumber; default = your session's newest open PR): open, merge, close, retarget, update, unlink an issue, files.`,
    alwaysLoad: true,
    ops: {
      open: `pr_open`,
      merge: `pr_merge`,
      close: `pr_close`,
      retarget: `pr_retarget`,
      update: `pr_update`,
      unlink: `pr_unlink`,
      files: `pr_files`,
    },
  },
  {
    name: `devices`,
    blurb: `Devices (machines running sessions): list, account_login (sign a device's agent login in).`,
    ops: { list: `devices_list`, account_login: `devices_account_login` },
  },
  {
    name: `events`,
    blurb: `The team's event stream: emit one (every agent with a trigger on that name wakes), list recent ones.`,
    ops: { emit: `events_emit`, list: `events_list` },
  },
  {
    name: `notifications`,
    blurb: `Your notifications: list, mark_read (one or all).`,
    ops: { list: `notifications_list`, mark_read: `notifications_mark_read` },
  },
]

/** The display row (`contract.expToolDisplay.tools[].name`) of every op
 *  and verb, in surface order: `<noun>_<op>`, the verbs by their own name.
 *  THE list the contract tables are checked against (preview.test.ts,
 *  tools.test.ts) and what the engine composes a call's row name from. */
export function displayRowNames(): string[] {
  const rows: string[] = []
  for (const spec of NOUN_TOOLS) {
    if (spec.verb) rows.push(spec.name)
    for (const op of Object.keys(spec.ops ?? {})) rows.push(`${spec.name}_${op}`)
  }
  return rows
}

/** The recorded (EXP-988) tool behind a display row, for the tests that
 *  drive one op's handler by its row name. */
export function legacyNameOfRow(row: string): string | null {
  for (const spec of NOUN_TOOLS) {
    if (spec.verb && spec.name === row) return `${PREFIX}${spec.verb}`
    for (const [op, name] of Object.entries(spec.ops ?? {})) {
      if (`${spec.name}_${op}` === row) return `${PREFIX}${name}`
    }
  }
  return null
}

/** Every recorded (EXP-988) tool this caller gets, by wire name. */
export function collectLegacyTools(
  user: McpUser,
  request: Request,
  access: McpAccess,
  sessionId: string | null = null,
  gates: McpToolGates = ALL_MCP_TOOL_GATES
): Map<string, LegacyTool> {
  const tools = new Map<string, LegacyTool>()
  const recorder = {
    registerTool: (name: string, def: LegacyToolDef, handler: LegacyHandler) => {
      tools.set(name, { name, def, handler })
    },
  }
  registerExponentialTools(recorder as never, user, request, access, sessionId, gates)
  return tools
}

/** `op{a, b?, c?}`: the op's parameters off its strict schema (required
 *  first, optional with `?`), the one place an agent learns them. */
export function opSignature(op: string, schema: z.ZodType | undefined): string {
  if (!schema) return `${op}{}`
  const json = z.toJSONSchema(schema, { io: `input`, target: `draft-7` }) as {
    properties?: Record<string, unknown>
    required?: string[]
  }
  const required = new Set(json.required ?? [])
  const keys = Object.keys(json.properties ?? {})
  const parts = [
    ...keys.filter((key) => required.has(key)),
    ...keys.filter((key) => !required.has(key)).map((key) => `${key}?`),
  ]
  return `${op}{${parts.join(`, `)}}`
}

/** The noun's description: its blurb, then every op with its parameters.
 *  An op the caller lacks is still listed (the surface is the same for
 *  everyone); calling it says why it is unavailable. */
export function nounDescription(
  spec: NounSpec,
  legacy: Map<string, LegacyTool>,
  everything: Map<string, LegacyTool>
): string {
  const ops = Object.entries(spec.ops ?? {}).map(([op, name]) => {
    const tool = legacy.get(`${PREFIX}${name}`) ?? everything.get(`${PREFIX}${name}`)
    return opSignature(op, tool?.def.inputSchema)
  })
  return `${spec.blurb} op = ${ops.join(` | `)}`
}

function unavailable(spec: NounSpec, op: string): string {
  if (spec.name === `session_self` || op === `end`) {
    return `${op} only works inside a session started by the Exponential launcher (missing X-Exp-Session-Id).`
  }
  return `${spec.name}.${op} is not available on this instance.`
}

/** Registers the 15 on the server for this caller. */
export function registerNounTools(
  server: McpServer,
  user: McpUser,
  request: Request,
  access: McpAccess,
  sessionId: string | null = null,
  gates: McpToolGates = ALL_MCP_TOOL_GATES
) {
  const legacy = collectLegacyTools(user, request, access, sessionId, gates)
  // The worst-case surface, so every op's parameters are described for
  // every caller (descriptions never depend on the gates).
  const everything =
    gates === ALL_MCP_TOOL_GATES
      ? legacy
      : collectLegacyTools(user, request, access, sessionId, ALL_MCP_TOOL_GATES)
  for (const spec of NOUN_TOOLS) {
    if (spec.verb) {
      // A verb this caller lacks (the Guide outside a session) still
      // registers with its definition; calling it says why.
      const tool = legacy.get(`${PREFIX}${spec.verb}`)
      const shape = tool ?? everything.get(`${PREFIX}${spec.verb}`)
      if (!shape) continue
      const def: LegacyToolDef = {
        ...shape.def,
        ...(spec.alwaysLoad ? { _meta: { ...(shape.def._meta ?? {}), ...ALWAYS_LOAD_META } } : {}),
      }
      const handler: LegacyHandler = tool
        ? tool.handler
        : async () => err(new Error(`${spec.name} only works inside a session started by the Exponential launcher (missing X-Exp-Session-Id).`))
      server.registerTool(`${PREFIX}${spec.name}`, def as never, handler as never)
      continue
    }
    const ops = Object.keys(spec.ops ?? {})
    const routes = new Map(
      Object.entries(spec.ops ?? {}).map(([op, name]) => [op, legacy.get(`${PREFIX}${name}`) ?? null])
    )
    // The ONE `_meta` of the noun: always-load, plus an op's MCP Apps view
    // binding (`issues_show`'s), which the host opens for the noun's results.
    const viewMeta = [...routes.values()].find((tool) => tool?.def._meta?.ui)?.def._meta
    const meta = {
      ...(viewMeta ?? {}),
      ...(spec.alwaysLoad ? ALWAYS_LOAD_META : {}),
    }
    server.registerTool(
      `${PREFIX}${spec.name}`,
      {
        description: nounDescription(spec, legacy, everything),
        inputSchema: z.object({ op: z.enum(ops as [string, ...string[]]) }).passthrough(),
        ...(Object.keys(meta).length > 0 ? { _meta: meta } : {}),
      } as never,
      (async (args: Record<string, unknown>, extra: unknown) => {
        const { op, ...rest } = args
        const tool = routes.get(String(op))
        if (tool === undefined) {
          return err(new Error(`Unknown op "${String(op)}" for ${spec.name}; one of ${ops.join(`, `)}.`))
        }
        if (tool === null) return err(new Error(unavailable(spec, String(op))))
        const schema = tool.def.inputSchema ?? z.strictObject({})
        const parsed = schema.safeParse(rest)
        if (!parsed.success) {
          const issues = parsed.error.issues
            .map((issue) => `${issue.path.join(`.`) || `(root)`}: ${issue.message}`)
            .join(`; `)
          return err(new Error(`Invalid arguments for ${spec.name}.${String(op)}: ${issues}`))
        }
        return tool.handler(parsed.data as Record<string, unknown>, extra)
      }) as never
    )
  }
}
