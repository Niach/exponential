import { beforeEach, describe, expect, it, vi } from "vitest"
import { z } from "zod"

// #EXP-1798: the guard rails on every destructive op (@exp/emcp
// deletes.json): the noun router refuses a non-single id and the burst past
// the limit before the op's handler runs, posts ONE `mcp.delete_burst_blocked`
// per window, and the audit row names the session and the deleted id.

const hoisted = vi.hoisted(() => ({
  deleted: [] as unknown[],
  emitted: [] as Array<{ teamId: string; name: string; payload: Record<string, unknown>; emittedBy: unknown }>,
}))

vi.mock(`./tools`, () => ({
  registerExponentialTools: (server: { registerTool: (name: string, def: unknown, handler: unknown) => void }) => {
    server.registerTool(
      `exponential_issues_get`,
      { description: `Get.`, inputSchema: z.strictObject({ id: z.string() }), annotations: { readOnlyHint: true } },
      async () => ({ content: [{ type: `text`, text: `{}` }] })
    )
    server.registerTool(
      `exponential_issues_update`,
      { description: `Update.`, inputSchema: z.strictObject({ id: z.string() }) },
      async () => ({ content: [{ type: `text`, text: `updated` }] })
    )
    server.registerTool(
      `exponential_issues_delete`,
      { description: `Delete.`, inputSchema: z.strictObject({ id: z.string().min(1) }) },
      async (args: { id: string }) => {
        hoisted.deleted.push(args.id)
        return { content: [{ type: `text`, text: `deleted ${args.id}` }] }
      }
    )
    server.registerTool(
      `exponential_apps_delete`,
      { description: `Trash an app.`, inputSchema: z.strictObject({ slug: z.string() }) },
      async () => ({ content: [{ type: `text`, text: `trashed` }] })
    )
  },
}))

vi.mock(`@/db/connection`, () => {
  const chain = {
    from: () => chain,
    where: () => chain,
    limit: async () => [{ teamId: `session-team` }],
  }
  return { db: { select: () => chain, transaction: async (fn: (tx: unknown) => unknown) => fn({}) } }
})
vi.mock(`@/lib/events`, () => ({
  emitEventInTx: async (_tx: unknown, args: (typeof hoisted.emitted)[number]) => {
    hoisted.emitted.push(args)
    return args
  },
}))
vi.mock(`@/lib/auth/membership`, () => ({ getPersonalTeamId: async () => `personal-team` }))

import fixture from "@exp/emcp/fixtures/deletes.json" with { type: "json" }
import auditFixture from "@exp/emcp/fixtures/audit.json" with { type: "json" }
import { DELETE_DESCRIPTION_LINE, DELETE_INSTRUCTIONS_LINE } from "@exp/emcp/deletes"
import { deleteBurstKey, guardDestructiveCall, resetDeleteBurstWindows } from "./delete-guard"
import { NOUN_TOOLS, registerNounTools } from "./nouns"
import { auditRows, classifyToolResponse } from "./audit"
import { ALL_MCP_TOOL_GATES } from "./gates"
import { MCP_SERVER_INSTRUCTIONS, mcpServerInstructions } from "./instructions"
import { FULL_ACCESS } from "./scope"
import type { McpUser } from "./server"

type Result = { content: { text?: string }[]; isError?: boolean }
type Handler = (args: Record<string, unknown>) => Promise<Result>

function surface(sessionId: string | null = `s-1`, authorization?: string) {
  const tools = new Map<string, { def: Record<string, unknown>; handler: Handler }>()
  const server = {
    registerTool: (name: string, def: Record<string, unknown>, handler: Handler) => tools.set(name, { def, handler }),
  }
  const request = new Request(`https://x.test/api/mcp`, {
    headers: authorization ? { authorization } : {},
  })
  const gates = sessionId ? ALL_MCP_TOOL_GATES : { inSession: false, unattended: false, agent: false }
  registerNounTools(server as never, { id: `u` } as unknown as McpUser, request, FULL_ACCESS, sessionId, gates)
  return tools
}

const text = (result: Result) => result.content[0]?.text ?? ``
const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

beforeEach(() => {
  resetDeleteBurstWindows()
  hoisted.deleted.length = 0
  hoisted.emitted.length = 0
})

describe(`deletes.json replay`, () => {
  it(`takes ONE explicit id, nothing else`, () => {
    for (const entry of fixture.singleIdCases) {
      const refused = guardDestructiveCall({
        noun: `issues`,
        op: `delete`,
        args: { id: entry.value },
        userId: `u`,
        verifiedSessionId: `s-${JSON.stringify(entry.value)}`,
        authorization: null,
        access: FULL_ACCESS,
      })
      expect(refused === null ? null : refused.includes(`ONE explicit`) ? `notOne` : `missing`, JSON.stringify(entry.value)).toBe(
        entry.expected
      )
    }
  })

  it(`admits a burst as the fixture does, refusing with the fixture's words`, () => {
    for (const entry of fixture.burstCases) {
      resetDeleteBurstWindows()
      const env = {
        [fixture.burst.env.max]: String(entry.max),
        [fixture.burst.env.windowSeconds]: String(entry.windowSeconds),
      }
      const verdicts = entry.at.map((s) =>
        guardDestructiveCall({
          noun: `labels`,
          op: `delete`,
          args: { id: `L` },
          userId: `u`,
          verifiedSessionId: `s`,
          authorization: null,
          access: FULL_ACCESS,
          now: s * 1000,
          env,
        })
      )
      expect(verdicts.map((v) => (v === null ? `ok` : `refused`)), entry.name).toEqual(
        entry.expected.map((e) => (e === `ok` ? `ok` : `refused`))
      )
    }
  })

  it(`leaves every other op alone`, () => {
    for (let i = 0; i < 20; i++) {
      expect(
        guardDestructiveCall({
          noun: `issues`,
          op: `update`,
          args: { id: `EXP-1,EXP-2` },
          userId: `u`,
          verifiedSessionId: `s`,
          authorization: null,
          access: FULL_ACCESS,
        })
      ).toBeNull()
    }
  })

  it(`keys a session by its id, else the credential`, () => {
    expect(deleteBurstKey(`u`, `s-1`, `Bearer a`)).toBe(`session:s-1`)
    expect(deleteBurstKey(`u`, null, `Bearer a`)).not.toBe(deleteBurstKey(`u`, null, `Bearer b`))
    expect(deleteBurstKey(`u`, null, null)).toBe(`user:u|token:cookie`)
  })
})

describe(`the noun router`, () => {
  it(`refuses a list of ids before the handler runs`, async () => {
    const write = surface().get(`exponential_issues_write`)!
    const result = await write.handler({ op: `delete`, id: `EXP-1,EXP-2` })
    expect(result.isError).toBe(true)
    expect(text(result)).toContain(`exactly ONE explicit id`)
    expect(hoisted.deleted).toEqual([])
  })

  it(`stops the 2026-11-12 loop at the limit and posts the event once`, async () => {
    const write = surface().get(`exponential_issues_write`)!
    const results: Result[] = []
    for (let i = 1; i <= 16; i++) results.push(await write.handler({ op: `delete`, id: `FEED-${i}` }))
    await flush()
    expect(hoisted.deleted).toEqual([`FEED-1`, `FEED-2`, `FEED-3`, `FEED-4`, `FEED-5`])
    expect(results.slice(5).every((r) => r.isError && text(r).startsWith(`Refused: more than 5 deletes within 60 s`))).toBe(true)
    expect(hoisted.emitted).toHaveLength(1)
    const [event] = hoisted.emitted
    expect(event!.name).toBe(fixture.burst.event)
    expect(event!.teamId).toBe(`session-team`)
    expect(event!.emittedBy).toEqual({ server: true })
    expect(Object.keys(event!.payload).sort()).toEqual([...fixture.eventPayload].sort())
    expect(event!.payload).toMatchObject({ op: `delete`, target: `delete:FEED-6`, sessionId: `s-1`, max: 5 })
    // The refusal audits as a failed call with its own code.
    expect(classifyToolResponse({ result: results[5] })).toEqual({
      outcome: { error: true },
      error: fixture.auditError,
    })
  })

  it(`counts deletes across nouns, per credential outside a session`, async () => {
    const a = surface(null, `Bearer a`)
    for (let i = 0; i < 5; i++) expect((await a.get(`exponential_issues_write`)!.handler({ op: `delete`, id: `X-${i}` })).isError).toBeUndefined()
    expect((await a.get(`exponential_apps`)!.handler({ op: `delete`, slug: `standup` })).isError).toBe(true)
    // Another token has its own window.
    const b = surface(null, `Bearer b`)
    expect((await b.get(`exponential_apps`)!.handler({ op: `delete`, slug: `standup` })).isError).toBeUndefined()
    await flush()
    expect(hoisted.emitted.map((e) => e.teamId)).toEqual([`personal-team`])
  })

  it(`says the rule on every tool carrying a delete, and in the instructions`, () => {
    const tools = surface()
    expect(tools.get(`exponential_issues_write`)!.def.description).toContain(DELETE_DESCRIPTION_LINE)
    expect(tools.get(`exponential_issues`)!.def.description).not.toContain(DELETE_DESCRIPTION_LINE)
    expect(tools.get(`exponential_apps`)!.def.description).toContain(DELETE_DESCRIPTION_LINE)
    expect(tools.get(`exponential_teams`)!.def.description).not.toContain(DELETE_DESCRIPTION_LINE)
    expect(MCP_SERVER_INSTRUCTIONS).toContain(DELETE_INSTRUCTIONS_LINE)
    expect(mcpServerInstructions({ inSession: false, unattended: false, agent: false, reportBug: false })).toContain(
      DELETE_INSTRUCTIONS_LINE
    )
  })

  it(`names every destructive op of the surface in the fixture`, () => {
    // A new delete-like op must join deletes.json (and so the guard).
    const destructive = /^(delete|remove|revoke|records_delete|invites_revoke)$/
    for (const spec of NOUN_TOOLS) {
      for (const op of Object.keys(spec.ops ?? {})) {
        if (destructive.test(op)) {
          expect((fixture.destructiveOps as Record<string, Record<string, string>>)[spec.name]?.[op], `${spec.name}.${op}`).toBeTruthy()
        }
      }
    }
  })
})

describe(`the audit row of a delete`, () => {
  it(`carries the session and the deleted id`, () => {
    const body = {
      jsonrpc: `2.0`,
      id: 1,
      method: `tools/call`,
      params: { name: `exponential_issues_write`, arguments: { op: `delete`, id: `EXP-12` } },
    }
    const answer = { jsonrpc: `2.0`, id: 1, result: { content: [{ type: `text`, text: `{}` }] } }
    const [row] = auditRows({ userId: `u`, clientId: `c`, teamId: null, appId: null, sessionId: `s-1` }, body, answer, 1)
    expect(row).toMatchObject({ userId: `u`, clientId: `c`, sessionId: `s-1`, target: `delete:EXP-12`, status: `ok` })
    expect(auditFixture.columns).toEqual(expect.arrayContaining([`session_id`, `target`]))
    for (const entry of fixture.targetCases) {
      const [r] = auditRows(
        { userId: `u`, clientId: null, teamId: null, appId: null },
        { ...body, params: { name: entry.tool, arguments: entry.args } },
        answer,
        1
      )
      expect(r!.target, entry.tool).toBe(entry.expected)
      expect(r!.sessionId).toBeNull()
    }
  })
})
