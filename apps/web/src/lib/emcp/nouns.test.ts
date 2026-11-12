import { describe, expect, it, vi } from "vitest"
import { z } from "zod"

// NEW-4: the noun router — 22 tools (EXP-1657: 7 groups), `op` routes to the per-operation
// definitions, their STRICT schemas validate the flat arguments, an op the
// caller lacks errors instead of vanishing, the display rows are the
// contract's. The per-operation handlers themselves are tools.test.ts's.

const recorded = vi.hoisted(() => ({ gatesSeen: [] as unknown[] }))

vi.mock(`./tools`, () => ({
  registerExponentialTools: (
    server: { registerTool: (name: string, def: unknown, handler: unknown) => void },
    _user: unknown,
    _request: unknown,
    _access: unknown,
    _sessionId: unknown,
    gates: { inSession: boolean }
  ) => {
    recorded.gatesSeen.push(gates)
    server.registerTool(
      `exponential_issues_get`,
      {
        description: `Get one issue.`,
        inputSchema: z.strictObject({ id: z.string(), commentsLimit: z.number().optional() }),
        annotations: { readOnlyHint: true },
        _meta: { "anthropic/alwaysLoad": true },
      },
      async (args: Record<string, unknown>) => ({ content: [{ type: `text`, text: JSON.stringify({ got: args }) }] })
    )
    server.registerTool(
      `exponential_issues_list`,
      {
        description: `List.`,
        annotations: { readOnlyHint: true },
        inputSchema: z.strictObject({
          limit: z.number().int().min(1).max(200).default(50),
          includeClosed: z.boolean().default(false),
          search: z.string().optional(),
          labelIds: z.array(z.string()).optional(),
          answer: z.record(z.string(), z.unknown()).optional(),
        }),
      },
      async (args: Record<string, unknown>) => ({ content: [{ type: `text`, text: JSON.stringify(args) }] })
    )
    server.registerTool(
      `exponential_issues_create`,
      { description: `Create.`, inputSchema: z.strictObject({ title: z.string() }) },
      async (args: Record<string, unknown>) => ({ content: [{ type: `text`, text: JSON.stringify({ created: args }) }] })
    )
    if (gates.inSession) {
      server.registerTool(
        `exponential_sessions_compact`,
        { description: `Compact.`, inputSchema: z.strictObject({}) },
        async () => ({ content: [{ type: `text`, text: `compacted` }] })
      )
      server.registerTool(
        `exponential_sessions_quiet`,
        { description: `Quiet.`, inputSchema: z.strictObject({}) },
        async () => ({ content: [{ type: `text`, text: `quiet` }] })
      )
      server.registerTool(
        `exponential_show`,
        { description: `Show.`, inputSchema: z.strictObject({ replace: z.boolean().optional() }) },
        async () => ({ content: [{ type: `text`, text: `shown` }] })
      )
    }
    server.registerTool(
      `exponential_sessions_send`,
      { description: `Send.`, inputSchema: z.strictObject({ text: z.string().optional() }) },
      async () => ({ content: [{ type: `text`, text: `sent` }] })
    )
  },
}))

import { contract } from "@exp/domain-contract"
import {
  coerceScalars,
  displayRowNames,
  legacyNameOfRow,
  NOUN_GROUPS,
  NOUN_TOOLS,
  opSignature,
  registerNounTools,
  RETIRED_ROW_ALIASES,
  scalarKinds,
} from "./nouns"
import { ALL_MCP_TOOL_GATES } from "./gates"
import { FULL_ACCESS, NO_ACCESS, type McpAccess } from "./scope"
import type { McpUser } from "./server"

type Handler = (args: Record<string, unknown>, extra?: unknown) => Promise<{ content: { text?: string }[]; isError?: boolean }>

function surface(gates = ALL_MCP_TOOL_GATES, access: McpAccess = FULL_ACCESS) {
  const tools = new Map<string, { def: Record<string, unknown>; handler: Handler }>()
  const server = {
    registerTool: (name: string, def: Record<string, unknown>, handler: Handler) => {
      tools.set(name, { def, handler })
    },
  }
  registerNounTools(server as never, { id: `u` } as unknown as McpUser, new Request(`https://x.test/api/mcp`), access, null, gates)
  return tools
}

const text = (result: { content: { text?: string }[] }) => result.content[0]?.text ?? ``

describe(`registerNounTools`, () => {
  it(`registers the 22 nouns and verbs, every caller alike`, () => {
    const names = [...surface().keys()]
    // #VAPP-293: a noun with reads AND writes registers both halves (the
    // mock records only issues' reads, so every other noun is all writes).
    expect(names).toEqual(
      NOUN_TOOLS.flatMap((spec) =>
        spec.name === `issues` ? [`exponential_issues`, `exponential_issues_write`] : [`exponential_${spec.name}`]
      )
    )
    expect(names).toHaveLength(23)
    expect(names).not.toContain(`exponential_team_config`)
    const headerless = [...surface({ inSession: false, unattended: false, agent: false }).keys()]
    expect(headerless).toEqual(names)
  })

  it(`routes op to the per-operation handler with its strict schema`, async () => {
    const issues = surface().get(`exponential_issues`)!
    expect(text(await issues.handler({ op: `get`, id: `EXP-1` }))).toBe(`{"got":{"id":"EXP-1"}}`)
    expect(text(await issues.handler({ op: `list` }))).toBe(`{"limit":50,"includeClosed":false}`)
    // An unknown key is still an error (EXP-705), never a silent drop.
    const unknown = await issues.handler({ op: `get`, id: `EXP-1`, bogus: 1 })
    expect(unknown.isError).toBe(true)
    expect(text(unknown)).toContain(`Invalid arguments for issues.get`)
    // A missing required parameter names itself.
    const missing = await issues.handler({ op: `get` })
    expect(text(missing)).toContain(`id`)
    // An unknown op lists the ops.
    const bad = await issues.handler({ op: `explode` })
    expect(bad.isError).toBe(true)
    expect(text(bad)).toContain(`Unknown op "explode"`)
  })

  // #VAPP-293: Claude's directory rejects a tool mixing reads and writes and
  // wants a title plus readOnlyHint or destructiveHint on every tool.
  it(`splits a noun into its read tool and its _write twin, annotated`, async () => {
    const tools = surface()
    const read = tools.get(`exponential_issues`)!
    const write = tools.get(`exponential_issues_write`)!
    expect(read.def.title).toBe(`Read issues`)
    expect(read.def.annotations).toEqual({ title: `Read issues`, readOnlyHint: true })
    expect(write.def.title).toBe(`Change issues`)
    expect(write.def.annotations).toMatchObject({ title: `Change issues`, readOnlyHint: false, destructiveHint: true })
    expect(read.def.description).toContain(`Writes: exponential_issues_write.`)
    expect(read.def.description).not.toContain(`create{`)
    expect(write.def.description).toContain(`Reads: exponential_issues.`)
    expect(write.def.description).toContain(`create{title}`)
    expect(write.def.description).not.toContain(`get{`)
    expect(text(await write.handler({ op: `create`, title: `T` }))).toBe(`{"created":{"title":"T"}}`)
    // The twin takes writes only.
    const misplaced = await write.handler({ op: `get`, id: `EXP-1` })
    expect(misplaced.isError).toBe(true)
    expect(text(misplaced)).toContain(`issues.get is on exponential_issues: call exponential_issues {op: "get"}`)
    // Every tool carries a title and the applicable hint.
    for (const [name, tool] of tools) {
      const annotations = tool.def.annotations as Record<string, unknown> | undefined
      expect(typeof tool.def.title, name).toBe(`string`)
      expect(annotations?.title, name).toBe(tool.def.title)
      expect(annotations?.readOnlyHint === true || typeof annotations?.destructiveHint === `boolean`, name).toBe(true)
    }
    // A write op that only adds stays non-destructive; anything else is not.
    expect((tools.get(`exponential_sessions_send`)!.def.annotations as { destructiveHint: boolean }).destructiveHint).toBe(true)
    expect((tools.get(`exponential_show`)!.def.annotations as { destructiveHint: boolean }).destructiveHint).toBe(false)
  })

  it(`the read tool still routes a write for first-party callers, never for OAuth (#VAPP-293)`, async () => {
    const firstParty = surface().get(`exponential_issues`)!
    expect(text(await firstParty.handler({ op: `create`, title: `T` }))).toBe(`{"created":{"title":"T"}}`)
    const oauth = surface(ALL_MCP_TOOL_GATES, { ...NO_ACCESS, origin: `oauth` }).get(`exponential_issues`)!
    const refused = await oauth.handler({ op: `create`, title: `T` })
    expect(refused.isError).toBe(true)
    expect(text(refused)).toContain(`issues.create is on exponential_issues_write`)
    const schema = oauth.def.inputSchema as z.ZodType
    expect(schema.safeParse({ op: `create` }).error?.issues[0]?.message).toContain(`exponential_issues_write`)
    expect(schema.safeParse({ op: `get` }).success).toBe(true)
  })

  it(`an unknown op's enum refusal names the noun`, () => {
    const schema = surface().get(`exponential_session_self`)!.def.inputSchema as z.ZodType
    expect(schema.safeParse({ op: `nope` }).error?.issues[0]?.message).toContain(`Unknown op "nope" for session_self`)
  })

  it(`describes every op with its parameters, and only op in the JSON schema`, () => {
    const issues = surface().get(`exponential_issues`)!
    expect(issues.def.description).toContain(`get{id, commentsLimit?}`)
    expect(issues.def.description).toContain(`list{limit?, includeClosed?, search?, labelIds?, answer?}`)
    const json = z.toJSONSchema(issues.def.inputSchema as z.ZodType, { io: `input`, target: `draft-7` }) as {
      properties: Record<string, unknown>
      required: string[]
    }
    expect(Object.keys(json.properties)).toEqual([`op`])
    expect(json.required).toEqual([`op`])
    expect(issues.def._meta).toMatchObject({ "anthropic/alwaysLoad": true })
  })

  it(`errors on an op or verb the caller lacks instead of hiding it`, async () => {
    const headerless = surface({ inSession: false, unattended: false, agent: false })
    const self = headerless.get(`exponential_session_self`)!
    const compact = await self.handler({ op: `compact` })
    expect(compact.isError).toBe(true)
    expect(text(compact)).toContain(`only works inside a session`)
    const show = headerless.get(`exponential_show`)!
    expect(show.def.description).toBe(`Show.`)
    expect(text(await show.handler({}))).toContain(`only works inside a session`)
    // In a session the verb is the recorded tool itself.
    expect(text(await surface().get(`exponential_show`)!.handler({}))).toBe(`shown`)
    expect(text(await surface().get(`exponential_session_self`)!.handler({ op: `compact` }))).toBe(`compacted`)
    // FEED-227: quiet, the op a silent turn calls before ending with no text.
    expect(text(await surface().get(`exponential_session_self`)!.handler({ op: `quiet` }))).toBe(`quiet`)
  })

  it(`coerces a stringified number, boolean, array or object into what the op declares (NEW-44)`, async () => {
    const issues = surface().get(`exponential_issues`)!
    // Claude Code sends every field beyond `op` as a string.
    expect(text(await issues.handler({ op: `list`, limit: `20`, includeClosed: `true` }))).toBe(
      `{"limit":20,"includeClosed":true}`
    )
    expect(text(await issues.handler({ op: `get`, id: `EXP-1`, commentsLimit: `5` }))).toBe(
      `{"got":{"id":"EXP-1","commentsLimit":5}}`
    )
    // A string field stays a string, even a numeric one; a non-numeric
    // string for a number still fails with the schema's message.
    expect(text(await issues.handler({ op: `list`, search: `42` }))).toBe(`{"limit":50,"includeClosed":false,"search":"42"}`)
    const bad = await issues.handler({ op: `list`, limit: `many` })
    expect(bad.isError).toBe(true)
    expect(text(bad)).toContain(`limit`)
    // An array or object sent as its JSON text (pr.open's issueIds).
    expect(text(await issues.handler({ op: `list`, labelIds: `["a", "b"]`, answer: `{"k": 1}` }))).toBe(
      `{"limit":50,"includeClosed":false,"labelIds":["a","b"],"answer":{"k":1}}`
    )
    const broken = await issues.handler({ op: `list`, labelIds: `[a` })
    expect(broken.isError).toBe(true)
    expect(text(broken)).toContain(`labelIds`)
    const kinds = scalarKinds(
      z.strictObject({ n: z.number().int().positive().optional(), b: z.boolean(), s: z.string(), l: z.array(z.string()) })
    )
    expect([...kinds.keys()]).toEqual([`n`, `b`, `l`])
    expect(coerceScalars({ n: `7`, b: `false`, s: `true` }, kinds)).toEqual({ n: 7, b: false, s: `true` })
  })

  it(`opSignature reads required first, then optional with ?`, () => {
    expect(opSignature(`x`, z.strictObject({ b: z.string().optional(), a: z.string(), c: z.number().optional() }))).toBe(
      `x{a, b?, c?}`
    )
    expect(opSignature(`y`, undefined)).toBe(`y{}`)
  })

  it(`the display rows are exactly the contract's, each routing to a recorded tool name`, () => {
    const rows = displayRowNames()
    // EXP-1657: the retired team_config rows stay in the contract as aliases
    // (old transcripts render ×4); the live rows are exactly the surface's.
    const live = contract.expToolDisplay.tools.filter((row) => !(`aliasOf` in row))
    const aliases = contract.expToolDisplay.tools.filter((row) => `aliasOf` in row)
    expect([...rows].sort()).toEqual(live.map((row) => row.name).sort())
    expect(Object.fromEntries(aliases.map((row) => [row.name, (row as { aliasOf?: string }).aliasOf]))).toEqual(
      RETIRED_ROW_ALIASES
    )
    for (const target of Object.values(RETIRED_ROW_ALIASES)) expect(rows).toContain(target)
    const previewed = contract.expToolPreview.tools.map((row) => row.name)
    expect([...previewed].sort()).toEqual([...rows, ...Object.keys(RETIRED_ROW_ALIASES)].sort())
    for (const row of rows) expect(legacyNameOfRow(row), row).toMatch(/^exponential_[a-z_]+$/)
    expect(legacyNameOfRow(`issues_labels_add`)).toBe(`exponential_issue_labels_add`)
    expect(legacyNameOfRow(`sessions_send`)).toBe(`exponential_sessions_send`)
    expect(legacyNameOfRow(`nope`)).toBeNull()
    expect(legacyNameOfRow(`mcp_servers_create`)).toBe(`exponential_mcp_servers_add`)
    expect(legacyNameOfRow(`team_config_labels_list`)).toBeNull()
  })

  // EXP-1657: the groups are the contract's, in order, and every tool sits
  // in exactly the group its spec names (NOUN_TOOLS order = group order).
  it(`the tool groups are exactly the contract's`, () => {
    expect(contract.expToolDisplay.groups.map((group) => group.id)).toEqual([...NOUN_GROUPS])
    for (const group of contract.expToolDisplay.groups) {
      expect(group.tools, group.id).toEqual(NOUN_TOOLS.filter((spec) => spec.group === group.id).map((spec) => spec.name))
    }
    expect(NOUN_TOOLS.map((spec) => spec.name)).toEqual(contract.expToolDisplay.groups.flatMap((group) => group.tools))
    // EXP-1663: skills sits in Agents, between prompts and mcp_servers.
    expect(contract.expToolDisplay.groups.find((group) => group.id === `agents`)?.tools).toEqual([
      `agents`,
      `prompts`,
      `skills`,
      `mcp_servers`,
    ])
    expect(displayRowNames().filter((row) => row.startsWith(`skills_`))).toEqual([
      `skills_list`,
      `skills_get`,
      `skills_create`,
      `skills_update`,
      `skills_delete`,
    ])
    expect(contract.expToolDisplay.groups.find((group) => group.id === `work`)?.tools).toEqual(
      NOUN_TOOLS.filter((spec) => spec.alwaysLoad).map((spec) => spec.name)
    )
  })
})
