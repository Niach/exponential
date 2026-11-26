// @vitest-environment node
// #NEW-144: the MCP endpoint's protocol base over HTTP for 2025-06-18,
// 2025-11-25 and 2026-07-28 clients (packages/emcp/fixtures/protocol.json),
// and the MRTR helper (mrtr.ts) end to end through the SDK's modern handler.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { z } from "zod"
import { McpServer, type ServerContext } from "@modelcontextprotocol/server"
import fixture from "@exp/emcp/fixtures/protocol.json" with { type: "json" }
import { clientContextOf } from "@exp/emcp/protocol"

vi.mock(`@/lib/runtime-config`, () => ({ buildRuntimeConfig: () => ({}) }))
vi.mock(`./tools`, () => ({
  registerExponentialTools: (server: { registerTool: (name: string, def: unknown, handler: unknown) => void }) => {
    const answer = (value: unknown) => ({ content: [{ type: `text`, text: JSON.stringify(value) }] })
    server.registerTool(`exponential_issues_list`, { annotations: { readOnlyHint: true }, inputSchema: z.strictObject({ search: z.string().optional() }) }, async () => answer([]))
    // The context a handler sees (ctx.client, ctx.principal, ctx.call).
    server.registerTool(`exponential_issues_get`, { annotations: { readOnlyHint: true }, inputSchema: z.strictObject({ id: z.string() }) }, async (_args: unknown, ctx: { client: unknown; principal: unknown; call: unknown }) =>
      answer({ client: ctx.client, principal: ctx.principal, call: ctx.call })
    )
  },
}))

const { handleMcpPost } = await import(`./gate`)
const { createExponentialMcpServer } = await import(`./server`)
const { FULL_ACCESS } = await import(`./scope`)
const { SERVER_EXTENSIONS } = await import(`./extensions`)
const { askInput, callOf, elicitForm, memoryRedeemStore, mintState, openState, redeemInput, requestStateVerifier, setRedeemStore, verifyRequestState } = await import(`./mrtr`)
const { withEmcpContext, grantOf } = await import(`./request-context`)

const MODERN = `2026-07-28`
const URL_ = `https://x.test/api/mcp`

function envelope(capabilities: Record<string, unknown> = {}) {
  return {
    [fixture.metaKeys.protocolVersion]: MODERN,
    [fixture.metaKeys.clientInfo]: { name: `test`, version: `1` },
    [fixture.metaKeys.clientCapabilities]: capabilities,
  }
}

function post(message: Record<string, unknown>, version?: string): Request {
  const headers: Record<string, string> = { "content-type": `application/json`, accept: `application/json, text/event-stream` }
  if (version) headers[`mcp-protocol-version`] = version
  if (version === MODERN && typeof message.method === `string`) {
    headers[`mcp-method`] = message.method
    const name = (message.params as { name?: unknown } | undefined)?.name
    if (typeof name === `string`) headers[`mcp-name`] = name
  }
  return new Request(URL_, { method: `POST`, headers, body: JSON.stringify(message) })
}

type Rpc = { result?: Record<string, unknown> & { tools?: { name: string }[] }; error?: { code: number; message: string; data?: unknown } }

/** One POST through the gate's transport, the server built the way the gate builds it. */
async function send(message: Record<string, unknown>, version?: string, sessionId: string | null = null): Promise<{ status: number; body: Rpc }> {
  const request = post(message, version)
  const client = clientContextOf(message)
  const call = callOf(message)
  const server = createExponentialMcpServer({ id: `u1` } as never, request, FULL_ACCESS, sessionId, undefined, {
    client,
    principal: { userId: `u1`, grant: `oauth:c1` },
    ...(call ? { call } : {}),
  })
  const response = await handleMcpPost(server, request)
  const text = await response.text()
  return { status: response.status, body: (text ? JSON.parse(text) : {}) as Rpc }
}

const toolNames = (rpc: Rpc) => (rpc.result?.tools ?? []).map((tool) => tool.name).sort()

describe(`the three eras (#NEW-144)`, () => {
  for (const version of [`2025-06-18`, `2025-11-25`]) {
    it(`${version}: initialize answers, with the extensions map`, async () => {
      const { status, body } = await send({
        jsonrpc: `2.0`,
        id: 1,
        method: `initialize`,
        params: { protocolVersion: version, capabilities: {}, clientInfo: { name: `old`, version: `1` } },
      })
      expect(status).toBe(200)
      expect(body.result?.protocolVersion).toBe(version)
      expect((body.result?.capabilities as { extensions?: unknown }).extensions).toEqual(SERVER_EXTENSIONS)
      expect(body.result?.instructions).toEqual(expect.any(String))
    })

    it(`${version}: a later tools/list is served statelessly`, async () => {
      const { status, body } = await send({ jsonrpc: `2.0`, id: 2, method: `tools/list`, params: {} }, version)
      expect(status).toBe(200)
      expect(toolNames(body)).toContain(`exponential_issues`)
    })
  }

  it(`2026-07-28: server/discover answers the spec's keys`, async () => {
    const { status, body } = await send(fixture.spec.discoverRequest, MODERN)
    expect(status).toBe(200)
    const result = body.result!
    expect(Object.keys(result).sort()).toEqual([...fixture.spec.discoverResultKeys].sort())
    expect(result.resultType).toBe(`complete`)
    expect(result.supportedVersions).toContain(MODERN)
    expect((result.capabilities as { extensions?: unknown }).extensions).toEqual(SERVER_EXTENSIONS)
    expect((result._meta as Record<string, unknown>)[fixture.metaKeys.serverInfo]).toMatchObject({ name: `exponential` })
  })

  it(`2026-07-28: a session caller declares no extensions`, async () => {
    const { body } = await send(fixture.spec.discoverRequest, MODERN, `s1`)
    expect((body.result!.capabilities as { extensions?: unknown }).extensions).toBeUndefined()
  })

  it(`2026-07-28 lists the same tools as 2025`, async () => {
    const modern = await send({ jsonrpc: `2.0`, id: 3, method: `tools/list`, params: { _meta: envelope() } }, MODERN)
    const legacy = await send({ jsonrpc: `2.0`, id: 3, method: `tools/list`, params: {} }, `2025-11-25`)
    expect(modern.body.result?.resultType).toBe(`complete`)
    expect(toolNames(modern.body)).toEqual(toolNames(legacy.body))
    expect(JSON.stringify(modern.body.result?.tools)).toBe(JSON.stringify(legacy.body.result?.tools))
  })

  it(`an unsupported version answers -32022 with the supported ones`, async () => {
    const message = { jsonrpc: `2.0`, id: 4, method: `tools/list`, params: { _meta: { ...envelope(), [fixture.metaKeys.protocolVersion]: `1900-01-01` } } }
    const { body } = await send(message, `1900-01-01`)
    expect(body.error?.code).toBe(fixture.errors.unsupportedProtocolVersion)
    expect((body.error?.data as { supported: string[] }).supported).toContain(MODERN)
  })

  it(`a modern subscriptions/listen is refused, never a dangling stream`, async () => {
    const { status, body } = await send({ jsonrpc: `2.0`, id: 6, method: `subscriptions/listen`, params: { _meta: envelope() } }, MODERN)
    expect(status).toBe(200)
    expect(body.error?.code).toBe(-32601)
  })

  it(`every handler gets ctx.client, ctx.principal and ctx.call`, async () => {
    const caps = { elicitation: { form: {} }, extensions: { [fixture.extensionIds.tasks]: {} } }
    const args = { op: `get`, id: `EXP-1` }
    const { body } = await send({ jsonrpc: `2.0`, id: 5, method: `tools/call`, params: { name: `exponential_issues`, arguments: args, _meta: envelope(caps) } }, MODERN)
    const seen = JSON.parse((body.result as { content: { text: string }[] }).content[0]!.text)
    expect(seen.client).toMatchObject({ era: `modern`, protocolVersion: MODERN, elicitation: { form: true, url: false }, tasks: true })
    expect(seen.principal).toEqual({ userId: `u1`, grant: `oauth:c1` })
    expect(seen.call).toEqual({ method: `tools/call`, tool: `exponential_issues`, args })
  })
})

describe(`grantOf`, () => {
  it(`names the credential`, () => {
    expect(grantOf({ clientId: `c1`, keyId: `k1` })).toBe(`oauth:c1`)
    expect(grantOf({ clientId: null, keyId: `k1` })).toBe(`key:k1`)
    expect(grantOf({ clientId: null })).toBe(`member`)
  })
})

// A stand-in confirm tool on the SDK's modern handler: the MRTR round trip.
function confirmServer(userId: string, grant: string, message: Record<string, unknown>) {
  const principal = { userId, grant }
  const server = new McpServer({ name: `t`, version: `1` }, { requestState: { verify: requestStateVerifier({ principal, call: callOf(message) ?? undefined }) } })
  server.registerTool(`confirm`, { inputSchema: z.object({ id: z.string() }) }, (async (args: { id: string }, sdkCtx: ServerContext) => {
    const ctx = withEmcpContext(sdkCtx, {
      client: clientContextOf({ params: { _meta: (sdkCtx.mcpReq.envelope ?? {}) as Record<string, unknown> } }),
      principal,
      call: { tool: `confirm`, args },
    })
    const redeemed = redeemInput<{ id: string }>(ctx)
    const answer = redeemed?.answer(`sure`)
    if (answer?.action === `accept` && answer.content?.yes === true) {
      return { content: [{ type: `text` as const, text: `deleted ${redeemed!.data!.id}` }] }
    }
    if (redeemed) return { content: [{ type: `text` as const, text: `kept` }] }
    const ask = askInput(ctx, { inputRequests: { sure: elicitForm(`Delete ${args.id}?`, { type: `object`, properties: { yes: { type: `boolean` } }, required: [`yes`] }) }, data: { id: args.id } })
    return ask ?? { content: [{ type: `text` as const, text: `no way to ask` }] }
  }) as never)
  return server
}

let nextId = 100
async function call(userId: string, params: Record<string, unknown>, grant = `member`): Promise<Rpc> {
  const message = { jsonrpc: `2.0`, id: nextId++, method: `tools/call`, params }
  const response = await handleMcpPost(confirmServer(userId, grant, message), post(message, MODERN))
  return (await response.json()) as Rpc
}

describe(`MRTR (mrtr.ts)`, () => {
  beforeEach(() => {
    process.env.BETTER_AUTH_SECRET ??= `local-test-secret-local-test-secret`
    setRedeemStore(memoryRedeemStore())
  })
  const caps = { elicitation: {} }
  const first = { name: `confirm`, arguments: { id: `EXP-9` }, _meta: envelope(caps) }

  it(`asks, then redeems the answer once`, async () => {
    const asked = await call(`u1`, first)
    expect(asked.result?.resultType).toBe(`input_required`)
    const state = asked.result?.requestState as string
    expect(state.startsWith(fixture.mrtr.stateKey.prefix)).toBe(true)
    expect(state).not.toContain(`EXP-9`)
    expect(asked.result?.inputRequests).toEqual({ sure: elicitForm(`Delete EXP-9?`, { type: `object`, properties: { yes: { type: `boolean` } }, required: [`yes`] }) })

    const retry = { ...first, inputResponses: { sure: { action: `accept`, content: { yes: true } } }, requestState: state }
    const done = await call(`u1`, retry)
    expect(done.result?.resultType).toBe(`complete`)
    expect((done.result as { content: { text: string }[] }).content[0]!.text).toBe(`deleted EXP-9`)

    const replay = await call(`u1`, retry)
    expect(replay.error?.code).toBe(fixture.errors.invalidParams)
    expect(replay.error?.message).toBe(fixture.errors.invalidRequestState)
  })

  it(`a declined answer goes on without acting`, async () => {
    const asked = await call(`u1`, first)
    const done = await call(`u1`, { ...first, inputResponses: { sure: { action: `decline` } }, requestState: asked.result?.requestState })
    expect((done.result as { content: { text: string }[] }).content[0]!.text).toBe(`kept`)
  })

  it(`refuses another member, credential or arguments`, async () => {
    const asked = await call(`u1`, first)
    const state = asked.result?.requestState
    const answers = { sure: { action: `accept`, content: { yes: true } } }
    expect((await call(`u2`, { ...first, inputResponses: answers, requestState: state })).error?.message).toBe(fixture.errors.invalidRequestState)
    expect((await call(`u1`, { ...first, inputResponses: answers, requestState: state }, `oauth:other`)).error?.message).toBe(fixture.errors.invalidRequestState)
    expect((await call(`u1`, { ...first, arguments: { id: `EXP-10` }, inputResponses: answers, requestState: state })).error?.message).toBe(fixture.errors.invalidRequestState)
    // The refusals spent nothing: the right caller still redeems it.
    expect((await call(`u1`, { ...first, inputResponses: answers, requestState: state })).result?.resultType).toBe(`complete`)
  })

  it(`refuses a tampered or foreign state`, async () => {
    const asked = await call(`u1`, first)
    const state = asked.result?.requestState as string
    const tampered = `${state.slice(0, -2)}${state.endsWith(`A`) ? `B` : `A`}A`
    const answers = { sure: { action: `accept`, content: { yes: true } } }
    expect((await call(`u1`, { ...first, inputResponses: answers, requestState: tampered })).error?.message).toBe(fixture.errors.invalidRequestState)
    expect((await call(`u1`, { ...first, inputResponses: answers, requestState: `spike-state-1` })).error?.message).toBe(fixture.errors.invalidRequestState)
  })

  it(`an expired state is refused`, () => {
    const ctx = withEmcpContext({ mcpReq: { method: `tools/call` } }, { client: clientContextOf(null), principal: { userId: `u1`, grant: `member` }, call: { tool: `confirm`, args: { id: `EXP-9` } } })
    const wire = mintState(ctx, { id: `EXP-9` }, Date.now() - (fixture.mrtr.ttlSeconds + 1) * 1000)
    expect(openState(wire)?.tool).toBe(`confirm`)
    expect(() => verifyRequestState(wire, { userId: `u1`, grant: `member` }, { method: `tools/call`, tool: `confirm`, args: { id: `EXP-9` } })).toThrow(fixture.errors.invalidRequestState)
    // A raw string reaching a handler means no verifier ran: never trusted.
    const raw = withEmcpContext({ mcpReq: { requestState: () => wire } }, { client: clientContextOf(null), principal: { userId: `u1`, grant: `member` } })
    expect(() => redeemInput(raw)).toThrow(fixture.errors.invalidRequestState)
  })

  it(`never asks a client that did not declare elicitation, or a 2025 one`, async () => {
    const quiet = await call(`u1`, { ...first, _meta: envelope({}) })
    expect((quiet.result as { content: { text: string }[] }).content[0]!.text).toBe(`no way to ask`)
    const urlOnly = await call(`u1`, { ...first, _meta: envelope({ elicitation: { url: {} } }) })
    expect((urlOnly.result as { content: { text: string }[] }).content[0]!.text).toBe(`no way to ask`)
    const legacy = withEmcpContext({}, { client: clientContextOf({ method: `initialize`, params: { capabilities: { elicitation: {} } } }), principal: { userId: `u1`, grant: `member` }, call: { tool: `confirm`, args: {} } })
    expect(askInput(legacy, { inputRequests: { sure: elicitForm(`?`, { type: `object`, properties: {} }) } })).toBeNull()
  })
})
