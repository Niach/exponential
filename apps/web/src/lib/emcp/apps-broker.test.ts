// @vitest-environment node
import { describe, expect, it } from "vitest"
import { Client } from "@modelcontextprotocol/sdk/client/index.js"
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js"
import fixture from "@exp/emcp/fixtures/exposes.json" with { type: "json" }
import type { Expose } from "@exp/emcp/exposes"
import { auditRows } from "@/lib/emcp/audit"
import {
  argumentsError,
  brokerError,
  classifyAppToolResponse,
  createAppsMcpServer,
  toolDefinition,
  toolsFingerprint,
  type BrokerApp,
  type BrokerTool,
} from "@/lib/emcp/apps-broker"

// #VAPP-181: the app tool broker's listing, refusals and audit reading
// (the DB-backed runs are covered by the endpoint's callers).

const [listEntries, logComment, findCustomer] = fixture.validateCases[0]!.exposes as unknown as Expose[]
const app: BrokerApp = { id: `a1`, slug: `timesheet`, name: `Timesheet`, icon: null, color: null, teamId: `t1`, grantedTools: [`exponential.comments.create`], upstreams: [`m5`], level: `write` }
const tools: BrokerTool[] = [
  { name: `timesheet__list_entries`, app, expose: listEntries! },
  { name: `timesheet__log_comment`, app, expose: logComment! },
  { name: `timesheet__find_customer`, app, expose: findCustomer! },
]

async function connect(list: BrokerTool[]) {
  const server = createAppsMcpServer({ user: { id: `u1` } as never, request: new Request(`http://x/api/mcp/apps`), teamId: `t1`, teamSlug: `acme` }, list)
  const [clientSide, serverSide] = InMemoryTransport.createLinkedPair()
  const client = new Client({ name: `test`, version: `1.0.0` })
  await server.connect(serverSide)
  await client.connect(clientSide)
  return client
}

describe(`app tool broker (VAPP-181)`, () => {
  it(`lists each tool with its frozen words, schema, hints and provenance`, () => {
    expect(toolDefinition(tools[0]!)).toEqual({
      name: `timesheet__list_entries`,
      title: `List entries`,
      description: `The time entries of a day.`,
      inputSchema: listEntries!.input,
      annotations: { title: `List entries`, readOnlyHint: true, destructiveHint: false, openWorldHint: false },
      _meta: fixture.metaCases[0]!.expected,
    })
    expect(toolDefinition(tools[1]!).annotations).toMatchObject({ readOnlyHint: false, destructiveHint: false })
    expect(toolDefinition(tools[2]!)._meta).toEqual({ ...fixture.metaCases[2]!.expected, "at.exponential/app": `timesheet`, "at.exponential/tool": `find_customer` })
    expect(toolDefinition(tools[2]!).annotations?.openWorldHint).toBe(true)
  })

  it(`the fingerprint moves with what the caller is shown`, () => {
    expect(toolsFingerprint(tools)).toBe(toolsFingerprint([...tools]))
    expect(toolsFingerprint(tools.slice(1))).not.toBe(toolsFingerprint(tools))
    const renamed = { ...tools[0]!, expose: { ...listEntries!, description: `Changed.` } }
    expect(toolsFingerprint([renamed, ...tools.slice(1)])).not.toBe(toolsFingerprint(tools))
  })

  it(`arguments are an object holding every required key`, () => {
    expect(argumentsError(logComment!, { issue: `X-1`, hours: 2 })).toBeNull()
    expect(argumentsError(logComment!, { issue: `X-1` })).toBe(`missing hours`)
    expect(argumentsError(logComment!, [])).toBe(`arguments are an object`)
  })

  it(`tools/list over MCP; an unlisted tool is MCP's unknown tool; bad arguments refuse before anything runs`, async () => {
    const client = await connect(tools)
    const listed = await client.listTools()
    expect(listed.tools.map((t) => t.name)).toEqual(tools.map((t) => t.name))
    await expect(client.callTool({ name: `timesheet__nope`, arguments: {} })).rejects.toThrow(/Tool timesheet__nope not found/)
    const invalid = await client.callTool({ name: `timesheet__log_comment`, arguments: { issue: `X-1` } })
    expect(invalid).toMatchObject({ isError: true, content: [{ type: `text`, text: `invalidArguments: missing hours` }], _meta: { "at.exponential/tool": `log_comment` } })
    await client.close()
  })

  it(`the audit reads every broker answer`, () => {
    const ok = { jsonrpc: `2.0`, id: 1, result: { content: [], _meta: {} } }
    const as = (code: string) => ({ jsonrpc: `2.0`, id: 1, result: brokerError(tools[2]!, code, `words`) })
    expect(classifyAppToolResponse(ok)).toEqual({ outcome: {}, error: null })
    expect(classifyAppToolResponse(as(`invalidArguments`))).toEqual({ outcome: { invalid: true }, error: `invalidArguments` })
    expect(classifyAppToolResponse(as(`notGranted`))).toEqual({ outcome: { denied: true }, error: `notGranted` })
    expect(classifyAppToolResponse(as(`destructive`))).toEqual({ outcome: { denied: true }, error: `destructive` })
    expect(classifyAppToolResponse(as(`upstreamDisconnected`))).toEqual({ outcome: { upstreamError: true }, error: `upstreamDisconnected` })
    expect(classifyAppToolResponse(as(`toolError`))).toEqual({ outcome: { error: true }, error: `toolError` })
    const upstreamOwn = { jsonrpc: `2.0`, id: 1, result: { isError: true, content: [{ type: `text`, text: `m5 said no` }], _meta: { "at.exponential/via": `upstream` } } }
    expect(classifyAppToolResponse(upstreamOwn)).toEqual({ outcome: { upstreamError: true }, error: `upstreamFailed` })
    const unknown = { jsonrpc: `2.0`, id: 1, error: { code: -32602, message: `MCP error -32602: Tool x__y not found` } }
    expect(classifyAppToolResponse(unknown)).toEqual({ outcome: { denied: true }, error: `unknownTool` })

    const body = [
      { jsonrpc: `2.0`, id: 1, method: `tools/call`, params: { name: `timesheet__list_entries`, arguments: { day: `2026-10-31` } } },
      { jsonrpc: `2.0`, id: 2, method: `tools/call`, params: { name: `x__y`, arguments: {} } },
    ]
    const rows = auditRows({ userId: `u1`, clientId: `c1`, teamId: `t1`, appId: null }, body, [{ ...ok, id: 1 }, { ...unknown, id: 2 }], 4, {
      appIdOf: (name) => (name.startsWith(`timesheet__`) ? `a1` : null),
      classify: classifyAppToolResponse,
    })
    expect(rows.map((r) => [r.tool, r.appId, r.teamId, r.status, r.error])).toEqual([
      [`timesheet__list_entries`, `a1`, `t1`, `ok`, null],
      [`x__y`, null, `t1`, `denied`, `unknownTool`],
    ])
  })
})
