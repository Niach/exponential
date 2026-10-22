import { describe, expect, it, vi } from "vitest"
import { z } from "zod"

// NEW-4: the noun router — 15 tools, `op` routes to the per-operation
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
        _meta: { "anthropic/alwaysLoad": true },
      },
      async (args: Record<string, unknown>) => ({ content: [{ type: `text`, text: JSON.stringify({ got: args }) }] })
    )
    server.registerTool(
      `exponential_issues_list`,
      {
        description: `List.`,
        inputSchema: z.strictObject({
          limit: z.number().int().min(1).max(200).default(50),
          includeClosed: z.boolean().default(false),
          search: z.string().optional(),
        }),
      },
      async (args: Record<string, unknown>) => ({ content: [{ type: `text`, text: JSON.stringify(args) }] })
    )
    if (gates.inSession) {
      server.registerTool(
        `exponential_sessions_compact`,
        { description: `Compact.`, inputSchema: z.strictObject({}) },
        async () => ({ content: [{ type: `text`, text: `compacted` }] })
      )
      server.registerTool(
        `exponential_guide_write`,
        { description: `Guide.`, inputSchema: z.strictObject({ topic: z.string() }) },
        async () => ({ content: [{ type: `text`, text: `written` }] })
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
  NOUN_TOOLS,
  opSignature,
  registerNounTools,
  scalarKinds,
} from "./nouns"
import { ALL_MCP_TOOL_GATES } from "./gates"
import { FULL_ACCESS } from "./scope"
import type { McpUser } from "./server"

type Handler = (args: Record<string, unknown>, extra?: unknown) => Promise<{ content: { text?: string }[]; isError?: boolean }>

function surface(gates = ALL_MCP_TOOL_GATES) {
  const tools = new Map<string, { def: Record<string, unknown>; handler: Handler }>()
  const server = {
    registerTool: (name: string, def: Record<string, unknown>, handler: Handler) => {
      tools.set(name, { def, handler })
    },
  }
  registerNounTools(server as never, { id: `u` } as unknown as McpUser, new Request(`https://x.test/api/mcp`), FULL_ACCESS, null, gates)
  return tools
}

const text = (result: { content: { text?: string }[] }) => result.content[0]?.text ?? ``

describe(`registerNounTools`, () => {
  it(`registers the 15 nouns, every caller alike`, () => {
    const names = [...surface().keys()]
    expect(names).toEqual(NOUN_TOOLS.map((spec) => `exponential_${spec.name}`))
    expect(names).toHaveLength(15)
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

  it(`describes every op with its parameters, and only op in the JSON schema`, () => {
    const issues = surface().get(`exponential_issues`)!
    expect(issues.def.description).toContain(`get{id, commentsLimit?}`)
    expect(issues.def.description).toContain(`list{limit?, includeClosed?, search?}`)
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
    const guide = headerless.get(`exponential_guide_write`)!
    expect(guide.def.description).toBe(`Guide.`)
    expect(text(await guide.handler({ topic: `Summary` }))).toContain(`only works inside a session`)
    // In a session the verb is the recorded tool itself.
    expect(text(await surface().get(`exponential_guide_write`)!.handler({ topic: `Summary` }))).toBe(`written`)
    expect(text(await surface().get(`exponential_session_self`)!.handler({ op: `compact` }))).toBe(`compacted`)
  })

  it(`coerces a stringified number or boolean into the scalar the op declares (NEW-44)`, async () => {
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
    const kinds = scalarKinds(z.strictObject({ n: z.number().int().positive().optional(), b: z.boolean(), s: z.string() }))
    expect([...kinds.keys()]).toEqual([`n`, `b`])
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
    expect([...rows].sort()).toEqual(contract.expToolDisplay.tools.map((row) => row.name).sort())
    expect([...rows].sort()).toEqual(contract.expToolPreview.tools.map((row) => row.name).sort())
    for (const row of rows) expect(legacyNameOfRow(row), row).toMatch(/^exponential_[a-z_]+$/)
    expect(legacyNameOfRow(`issues_labels_add`)).toBe(`exponential_issue_labels_add`)
    expect(legacyNameOfRow(`sessions_send`)).toBe(`exponential_sessions_send`)
    expect(legacyNameOfRow(`nope`)).toBeNull()
  })
})
