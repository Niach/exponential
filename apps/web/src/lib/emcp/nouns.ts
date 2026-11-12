import { z } from "zod"
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { registerExponentialTools } from "./tools"
import { ALWAYS_LOAD_META } from "./always-load"
import { ALL_MCP_TOOL_GATES, type McpToolGates } from "./gates"
import type { McpUser } from "./server"
import type { McpAccess } from "./scope"
import { err } from "./helpers"
import { compactViewResult, isViewListOp } from "./view-results"
import { guardDestructiveCall } from "./delete-guard"
import { DELETE_DESCRIPTION_LINE, destructiveKey } from "@exp/emcp/deletes"

// NEW-4 (plan 3.11): the MCP surface is 22 tools, the SAME set for every
// caller, no conditional registration. ONE tool per noun takes an `op` and
// that op's parameters flat beside it (`{op: "get", id: "EXP-42"}`); two
// verbs stay whole (`sessions_send`, S-3's `show`). EXP-1657 split the old
// `team_config` into its nouns (labels, statuses, prompts, agents,
// mcp_servers, repositories) with uniform verbs, in 7 groups. The 79 per-operation
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
// #VAPP-293: on the wire a noun is TWO tools, its reads (`exponential_issues`,
// readOnlyHint) and its writes (`exponential_issues_write`), each with a
// title and its hints (Claude's connector directory rejects a tool mixing
// both); see `nounHalves`.
//
// Display ×4: a call's row in `contract.expToolDisplay.tools` is
// `<noun>_<op>` (`issues_get`, `people_invites_create`), composed by the
// engine from the call's name and its `op` (`crates/engine/src/mapper.rs`),
// so every client's lookup and copy stay keyed by that row name.

type ToolResult = { content: unknown[]; isError?: boolean; structuredContent?: unknown }
type LegacyHandler = (args: Record<string, unknown>, extra: unknown) => Promise<ToolResult>

export interface LegacyToolDef {
  title?: string
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
  /** What the tools act on, lower-case (`pull requests`): the read tool's
   *  title is `Read <label>`, the write twin's `Change <label>`; a verb's
   *  label IS its title. */
  label: string
  /** The first sentence of the description. */
  blurb: string
  /** `op` → the recorded (EXP-988) tool it routes to; absent for a verb. */
  ops?: Record<string, string>
  /** A verb tool registered whole under this name. */
  verb?: string
  alwaysLoad?: boolean
  /** EXP-1657: the group (`contract.expToolDisplay.groups[].id`). */
  group: NounGroup
}

const PREFIX = `exponential_`

/** EXP-1657: the groups the tools sit in, in order (`contract.expToolDisplay.groups`:
 *  Settings › MCP lists one band per group ×4). */
export const NOUN_GROUPS = [`work`, `tracker`, `team`, `agents`, `sessions`, `apps`, `activity`] as const
export type NounGroup = (typeof NOUN_GROUPS)[number]

/** The 22. Order = the order the tools list = the group order. */
export const NOUN_TOOLS: readonly NounSpec[] = [
  // ── Work (always loaded) ──
  {
    name: `issues`,
    label: `issues`,
    group: `work`,
    blurb: `Issues (id = UUID or identifier: EXP-12; list = open only unless includeClosed or a status filter; widget = reporter data). create links only via sessionId or link: true.`,
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
      unlink: `issues_unlink`,
      labels_add: `issue_labels_add`,
      labels_remove: `issue_labels_remove`,
      relations_add: `issue_relations_add`,
      relations_remove: `issue_relations_remove`,
    },
  },
  {
    name: `comments`,
    label: `comments`,
    group: `work`,
    blurb: `Comments on an issue (plain GFM; replies one level deep; audience reporter emails a widget reporter; a report's reporter sees only that report).`,
    alwaysLoad: true,
    ops: { list: `comments_list`, create: `comments_create`, update: `comments_update`, delete: `comments_delete` },
  },
  {
    name: `pr`,
    label: `pull requests`,
    group: `work`,
    blurb: `Pull requests (name one by pullRequestId, issueId or repositoryId + prNumber; default = your session's newest open PR): open, merge, close, retarget, update, unlink an issue, files. Every reply carries checks (the PR's CI); CI finishing wakes the PR's session, so never poll GitHub.`,
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
  { name: `sessions_send`, label: `Message or start a session`, group: `work`, blurb: ``, verb: `sessions_send`, alwaysLoad: true },
  // S-3: show / ask / plan = ONE card verb, always loaded.
  { name: `show`, label: `Show a card, question or plan`, group: `work`, blurb: ``, verb: `show`, alwaysLoad: true },
  // ── Tracker ──
  {
    name: `boards`,
    label: `boards`,
    group: `tracker`,
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
    name: `labels`,
    label: `labels`,
    group: `tracker`,
    blurb: `A team's issue labels: list, get, create, update, delete.`,
    ops: {
      list: `labels_list`,
      get: `labels_get`,
      create: `labels_create`,
      update: `labels_update`,
      delete: `labels_delete`,
    },
  },
  {
    name: `statuses`,
    label: `statuses`,
    group: `tracker`,
    blurb: `A team's issue statuses (each in an anchor category): list, create, update, delete.`,
    ops: {
      list: `statuses_list`,
      create: `statuses_create`,
      update: `statuses_update`,
      delete: `statuses_delete`,
    },
  },
  {
    name: `attachments`,
    label: `attachments`,
    group: `tracker`,
    blurb: `Files on an issue: list, get (metadata + contents or a signed URL), upload (bytes now, or a signed uploadUrl then finalize; no issueId = a loose file for sessions_send attachmentIds), delete.`,
    ops: {
      list: `attachments_list`,
      get: `attachments_get`,
      upload: `attachments_upload`,
      delete: `attachments_delete`,
    },
  },
  // ── Team ──
  {
    name: `teams`,
    label: `teams`,
    group: `team`,
    blurb: `Teams you are a member of: list, get, create, update.`,
    ops: { list: `teams_list`, get: `teams_get`, create: `teams_create`, update: `teams_update` },
  },
  {
    name: `people`,
    label: `people`,
    group: `team`,
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
    name: `repositories`,
    label: `repositories`,
    group: `team`,
    blurb: `A team's GitHub repositories: list, create (connect one the team's GitHub App can see).`,
    ops: { list: `repositories_list`, create: `repositories_add` },
  },
  {
    name: `devices`,
    label: `devices`,
    group: `team`,
    blurb: `Devices (machines running sessions): list, account_login (sign a device's agent login in).`,
    ops: { list: `devices_list`, account_login: `devices_account_login` },
  },
  // ── Agents (EXP-1663: + skills) ──
  {
    name: `agents`,
    label: `agents`,
    group: `agents`,
    blurb: `A team's agents (message one with exponential_sessions_send {agentId}): list, get, create, update, delete (owners write).`,
    ops: {
      list: `agents_list`,
      get: `agents_get`,
      create: `agents_create`,
      update: `agents_update`,
      delete: `agents_delete`,
    },
  },
  {
    name: `prompts`,
    label: `prompts`,
    group: `agents`,
    blurb: `A team's saved prompts (run one with exponential_sessions_send {promptId}): list, get, create, update, delete.`,
    ops: {
      list: `prompts_list`,
      get: `prompts_get`,
      create: `prompts_create`,
      update: `prompts_update`,
      delete: `prompts_delete`,
    },
  },
  {
    name: `skills`,
    label: `skills`,
    group: `agents`,
    blurb: `A team's skills (SKILL.md instructions a session can run with): list, get (by id or name), create, update, delete (owners write).`,
    ops: {
      list: `skills_list`,
      get: `skills_get`,
      create: `skills_create`,
      update: `skills_update`,
      delete: `skills_delete`,
    },
  },
  {
    name: `mcp_servers`,
    label: `MCP servers`,
    group: `agents`,
    blurb: `A team's MCP servers for coding sessions (never .mcp.json): list, create, delete.`,
    ops: { list: `mcp_servers_list`, create: `mcp_servers_add`, delete: `mcp_servers_remove` },
  },
  // ── Sessions ──
  {
    name: `sessions`,
    label: `sessions`,
    group: `sessions`,
    blurb: `Sessions (roots, agents, children): get a tree (no id = yours), list, link/unlink issues, configure (model, account, effort; no message), escalate a waiting card to the managing agent's conversation, end one.`,
    ops: {
      get: `sessions_get`,
      list: `sessions_list`,
      link: `sessions_link`,
      unlink: `sessions_unlink`,
      configure: `sessions_configure`,
      escalate: `sessions_escalate`,
      end: `sessions_end`,
    },
  },
  {
    name: `session_self`,
    label: `your own session`,
    group: `sessions`,
    blurb: `Your own session: suggest replies or tasks, compact your context, quiet (nothing to say: then end your turn with no text), report_bug when Exponential itself misbehaves (cards, questions and plans: exponential_show).`,
    ops: {
      suggest: `sessions_suggest`,
      compact: `sessions_compact`,
      quiet: `sessions_quiet`,
      report_bug: `report_bug`,
    },
  },
  // ── Apps ──
  {
    name: `apps`,
    label: `apps`,
    group: `apps`,
    blurb: `Exponential apps (a team's declarative genUI packages, rendered in every client): list, get (the head or a version's package), upsert (= a new DRAFT version; creates the app on a new slug), publish (members and the app's tools run the published version), delete, catalog (the components, bindings, functions and records a package may use), records_list / records_upsert / records_delete (an app's own data), records_history / records_restore (a record's 90-day history), handoff (run an app's agent hand-off).`,
    ops: {
      list: `apps_list`,
      get: `apps_get`,
      upsert: `apps_upsert`,
      publish: `apps_publish`,
      delete: `apps_delete`,
      catalog: `apps_catalog`,
      records_list: `apps_records_list`,
      records_upsert: `apps_records_upsert`,
      records_delete: `apps_records_delete`,
      records_history: `apps_records_history`,
      records_restore: `apps_records_restore`,
      handoff: `apps_handoff`,
    },
  },
  // ── Activity ──
  {
    name: `events`,
    label: `events`,
    group: `activity`,
    blurb: `The team's event stream: emit one (every agent with a trigger on that name wakes), list recent ones.`,
    ops: { emit: `events_emit`, list: `events_list` },
  },
  {
    name: `notifications`,
    label: `notifications`,
    group: `activity`,
    blurb: `Your notifications: list, mark_read (one or all).`,
    ops: { list: `notifications_list`, mark_read: `notifications_mark_read` },
  },
]

/** EXP-1657: the retired `team_config` tool's display rows → the row each
 *  became. The contract keeps the old rows (`aliasOf`) so a transcript from
 *  before the split still renders ×4; nothing registers them. */
export const RETIRED_ROW_ALIASES: Readonly<Record<string, string>> = {
  team_config_labels_list: `labels_list`,
  team_config_labels_get: `labels_get`,
  team_config_labels_create: `labels_create`,
  team_config_labels_update: `labels_update`,
  team_config_labels_delete: `labels_delete`,
  team_config_statuses_list: `statuses_list`,
  team_config_statuses_create: `statuses_create`,
  team_config_statuses_update: `statuses_update`,
  team_config_statuses_delete: `statuses_delete`,
  team_config_prompts_list: `prompts_list`,
  team_config_prompts_get: `prompts_get`,
  team_config_prompts_create: `prompts_create`,
  team_config_prompts_update: `prompts_update`,
  team_config_prompts_delete: `prompts_delete`,
  team_config_agents_list: `agents_list`,
  team_config_mcp_servers_list: `mcp_servers_list`,
  team_config_mcp_servers_add: `mcp_servers_create`,
  team_config_mcp_servers_remove: `mcp_servers_delete`,
  team_config_repositories_list: `repositories_list`,
  team_config_repositories_add: `repositories_create`,
}

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
  everything: Map<string, LegacyTool>,
  half?: { ops: readonly string[]; twin?: string; read: boolean }
): string {
  const ops = Object.entries(spec.ops ?? {}).filter(([op]) => !half || half.ops.includes(op)).map(([op, name]) => {
    const tool = legacy.get(`${PREFIX}${name}`) ?? everything.get(`${PREFIX}${name}`)
    return opSignature(op, tool?.def.inputSchema)
  })
  // #VAPP-293: each half names its twin, so a write is one hop away.
  const twin = half?.twin ? ` ${half.read ? `Writes` : `Reads`}: ${half.twin}.` : ``
  // #EXP-1798: every tool carrying a destructive op says the rule.
  const shown = half ? half.ops : Object.keys(spec.ops ?? {})
  const deletes = !half?.read && shown.some((op) => destructiveKey(spec.name, op)) ? ` ${DELETE_DESCRIPTION_LINE}` : ``
  return `${spec.blurb}${twin}${deletes} op = ${ops.join(` | `)}`
}

// NEW-44: a noun's JSON schema carries only `op`, so a client passes every
// other field untyped and Claude Code sends numbers and booleans as strings
// (`limit: "50"`, `prNumber: "1129"`, `includeClosed: "true"`) and arrays
// and objects as their JSON text (`issueIds: '["NEW-43"]'`). The ops'
// strict schemas reject those, so the router coerces a top-level string
// into the shape the op's schema declares before it parses: a numeric
// string where a number is expected, `true`/`false` where a boolean is,
// parsed JSON where an array or object is. Anything else (a non-numeric
// string, JSON that does not parse) reaches the schema as sent and fails
// as before.
type ScalarKinds = Map<string, Set<string>>

const COERCIBLE = new Set([`number`, `integer`, `boolean`, `array`, `object`])

function jsonTypes(node: unknown, into: Set<string>): void {
  if (!node || typeof node !== `object`) return
  const schema = node as { type?: string | string[]; anyOf?: unknown[]; oneOf?: unknown[] }
  if (typeof schema.type === `string`) into.add(schema.type)
  if (Array.isArray(schema.type)) for (const type of schema.type) into.add(type)
  for (const branch of [...(schema.anyOf ?? []), ...(schema.oneOf ?? [])]) jsonTypes(branch, into)
}

/** `field` → the JSON schema types its op declares, for the fields a string
 *  may be coerced into (number/integer, boolean, array, object). */
export function scalarKinds(schema: z.ZodType | undefined): ScalarKinds {
  const kinds: ScalarKinds = new Map()
  if (!schema) return kinds
  const json = z.toJSONSchema(schema, { io: `input`, target: `draft-7` }) as {
    properties?: Record<string, unknown>
  }
  for (const [key, node] of Object.entries(json.properties ?? {})) {
    const types = new Set<string>()
    jsonTypes(node, types)
    const scalar = new Set([...types].filter((type) => COERCIBLE.has(type)))
    if (scalar.size > 0) kinds.set(key, scalar)
  }
  return kinds
}

const NUMERIC = /^-?\d+(\.\d+)?$/

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text)
  } catch {
    return undefined
  }
}

/** The flat arguments with every stringified number, boolean, array or
 *  object turned into the value its op declares. */
export function coerceScalars(args: Record<string, unknown>, kinds: ScalarKinds): Record<string, unknown> {
  const out: Record<string, unknown> = { ...args }
  for (const [key, types] of kinds) {
    const value = out[key]
    if (typeof value !== `string`) continue
    if ((types.has(`number`) || types.has(`integer`)) && NUMERIC.test(value.trim())) {
      out[key] = Number(value.trim())
    } else if (types.has(`boolean`) && (value === `true` || value === `false`)) {
      out[key] = value === `true`
    } else if (types.has(`array`) && value.trimStart().startsWith(`[`)) {
      const parsed = parseJson(value)
      if (Array.isArray(parsed)) out[key] = parsed
    } else if (types.has(`object`) && value.trimStart().startsWith(`{`)) {
      const parsed = parseJson(value)
      if (parsed && typeof parsed === `object` && !Array.isArray(parsed)) out[key] = parsed
    }
  }
  return out
}

function unavailable(spec: NounSpec, op: string): string {
  if (spec.name === `session_self` || op === `end`) {
    return `${op} only works inside a session started by the Exponential launcher (missing X-Exp-Session-Id).`
  }
  return `${spec.name}.${op} is not available on this instance.`
}

function unknownOp(spec: NounSpec, op: string, ops: readonly string[], twin?: { name: string; ops: readonly string[] }): string {
  // #VAPP-293: an op that lives on the other half names its tool.
  if (twin?.ops.includes(op)) return `${spec.name}.${op} is on ${twin.name}: call ${twin.name} {op: "${op}"}.`
  return `Unknown op "${op}" for ${spec.name}; one of ${ops.join(`, `)}.`
}

// #VAPP-293 (Claude's connector directory): a tool that mixes reads and
// writes is rejected, and every tool needs a `title` plus `readOnlyHint` or
// `destructiveHint`. So each noun registers as TWO tools: its reads under
// the noun's own name (`exponential_issues`, readOnlyHint) and its writes
// under `exponential_<noun>_write` (a noun with no reads, `session_self`,
// keeps its name). The read/write split is the per-operation tools' own
// FEED-25 `readOnlyHint` (gated by api-conventions.test.ts). A write is
// destructive unless every op of the twin only ADDS something.
const ADDITIVE_OPS = new Set([
  `create`,
  `add`,
  `upload`,
  `emit`,
  `subscribe`,
  `invites_create`,
  `labels_add`,
  `relations_add`,
  `link`,
  `suggest`,
  `quiet`,
  `report_bug`,
])

export interface NounHalf {
  /** Wire name (`exponential_issues_write`). */
  name: string
  read: boolean
  ops: string[]
  title: string
  annotations: { title: string; readOnlyHint: boolean; destructiveHint?: boolean; openWorldHint?: boolean }
}

function capitalised(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1)
}

/** The tools a noun registers as: its read half, its write half, or both. */
export function nounHalves(spec: NounSpec, isRead: (op: string) => boolean): NounHalf[] {
  const ops = Object.keys(spec.ops ?? {})
  const reads = ops.filter(isRead)
  const writes = ops.filter((op) => !isRead(op))
  // A pull request's writes reach GitHub; everything else stays in Exponential.
  const openWorldHint = spec.name === `pr`
  const halves: NounHalf[] = []
  if (reads.length > 0) {
    const title = `Read ${spec.label}`
    halves.push({
      name: `${PREFIX}${spec.name}`,
      read: true,
      ops: reads,
      title,
      annotations: { title, readOnlyHint: true },
    })
  }
  if (writes.length > 0) {
    const title = reads.length > 0 ? `Change ${spec.label}` : capitalised(spec.label)
    halves.push({
      name: reads.length > 0 ? `${PREFIX}${spec.name}_write` : `${PREFIX}${spec.name}`,
      read: false,
      ops: writes,
      title,
      annotations: {
        title,
        readOnlyHint: false,
        destructiveHint: writes.some((op) => !ADDITIVE_OPS.has(op)),
        ...(openWorldHint ? { openWorldHint } : {}),
      },
    })
  }
  return halves
}

/** Every wire name the 22 nouns and verbs register as, in surface order. */
export function wireToolNames(isRead: (row: string) => boolean): string[] {
  return NOUN_TOOLS.flatMap((spec) =>
    spec.verb ? [`${PREFIX}${spec.name}`] : nounHalves(spec, (op) => isRead(`${spec.name}_${op}`)).map((half) => half.name)
  )
}

// The verbs' annotations: sending can stop a session; a card only adds one.
const VERB_ANNOTATIONS: Readonly<Record<string, { readOnlyHint: boolean; destructiveHint: boolean }>> = {
  sessions_send: { readOnlyHint: false, destructiveHint: true },
  show: { readOnlyHint: false, destructiveHint: false },
}

/** Registers the 22 on the server for this caller: a verb whole, a noun as
 *  its read tool and its `_write` twin (#VAPP-293). */
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
  // #VAPP-293: our own clients (a session, the apps, the desktop API) shipped
  // calling a write op on the noun's own name; for every caller but an OAuth
  // client (the Claude connector) the read tool still routes it. An OAuth
  // client's read tool takes reads only and names the twin for a write.
  const legacyWrites = access.origin !== `oauth`
  for (const spec of NOUN_TOOLS) {
    if (spec.verb) {
      // A verb this caller lacks (the Guide outside a session) still
      // registers with its definition; calling it says why.
      const tool = legacy.get(`${PREFIX}${spec.verb}`)
      const shape = tool ?? everything.get(`${PREFIX}${spec.verb}`)
      if (!shape) continue
      const def: LegacyToolDef = {
        ...shape.def,
        title: spec.label,
        annotations: { ...(shape.def.annotations ?? {}), title: spec.label, ...VERB_ANNOTATIONS[spec.name] },
        ...(spec.alwaysLoad ? { _meta: { ...(shape.def._meta ?? {}), ...ALWAYS_LOAD_META } } : {}),
      }
      const handler: LegacyHandler = tool
        ? tool.handler
        : async () => err(new Error(`${spec.name} only works inside a session started by the Exponential launcher (missing X-Exp-Session-Id).`))
      server.registerTool(`${PREFIX}${spec.name}`, def as never, handler as never)
      continue
    }
    const allOps = Object.keys(spec.ops ?? {})
    const routes = new Map(
      Object.entries(spec.ops ?? {}).map(([op, name]) => [op, legacy.get(`${PREFIX}${name}`) ?? null])
    )
    const isRead = (op: string) => {
      const name = spec.ops?.[op]
      const tool = name ? (legacy.get(`${PREFIX}${name}`) ?? everything.get(`${PREFIX}${name}`)) : undefined
      return tool?.def.annotations?.readOnlyHint === true
    }
    const kinds = new Map([...routes].map(([op, tool]) => [op, scalarKinds(tool?.def.inputSchema)]))
    const halves = nounHalves(spec, isRead)
    for (const half of halves) {
      const twin = halves.find((other) => other !== half)
      // The read tool of a first-party caller still accepts the writes.
      const accepted = half.read && legacyWrites ? allOps : half.ops
      // The ONE `_meta` of a half: always-load, plus an op's MCP Apps view
      // binding (`issues_show`'s), which the host opens for its results.
      const viewMeta = half.ops.map((op) => routes.get(op)).find((tool) => tool?.def._meta?.ui)?.def._meta
      const meta = {
        ...(viewMeta ?? {}),
        ...(spec.alwaysLoad ? ALWAYS_LOAD_META : {}),
      }
      const twinRef = twin ? { name: twin.name, ops: twin.ops } : undefined
      server.registerTool(
        half.name,
        {
          title: half.title,
          description: nounDescription(spec, legacy, everything, { ops: half.ops, twin: twin?.name, read: half.read }),
          // The op enum's own refusal names a moved op's new home (the SDK
          // validates before the handler runs).
          inputSchema: z
            .object({
              op: z.enum(accepted as [string, ...string[]], {
                error: (issue) =>
                  typeof issue.input === `string` ? unknownOp(spec, issue.input, half.ops, twinRef) : undefined,
              }),
            })
            .passthrough(),
          annotations: half.annotations,
          ...(Object.keys(meta).length > 0 ? { _meta: meta } : {}),
        } as never,
        (async (args: Record<string, unknown>, extra: unknown) => {
          const { op, ...rest } = args
          if (!accepted.includes(String(op))) return err(new Error(unknownOp(spec, String(op), half.ops, twinRef)))
          const tool = routes.get(String(op))
          if (tool === undefined) return err(new Error(unknownOp(spec, String(op), half.ops, twinRef)))
          if (tool === null) return err(new Error(unavailable(spec, String(op))))
          const schema = tool.def.inputSchema ?? z.strictObject({})
          const parsed = schema.safeParse(coerceScalars(rest, kinds.get(String(op)) ?? new Map()))
          if (!parsed.success) {
            const issues = parsed.error.issues
              .map((issue) => `${issue.path.join(`.`) || `(root)`}: ${issue.message}`)
              .join(`; `)
            return err(new Error(`Invalid arguments for ${spec.name}.${String(op)}: ${issues}`))
          }
          const data = parsed.data as Record<string, unknown>
          // #EXP-1798: one explicit id, within the burst (delete-guard.ts).
          const refused = guardDestructiveCall({
            noun: spec.name,
            op: String(op),
            args: data,
            userId: user.id,
            verifiedSessionId: gates.inSession ? sessionId : null,
            authorization: request.headers.get(`authorization`),
            access,
          })
          if (refused) return err(new Error(refused))
          const result = await tool.handler(data, extra)
          // #VAPP-292: outside a session a view list op answers the model with
          // a summary and the view with the rows (view-results.ts).
          return sessionId === null && isViewListOp(spec.name, String(op))
            ? compactViewResult(spec.name, data, result)
            : result
        }) as never
      )
    }
  }
}
