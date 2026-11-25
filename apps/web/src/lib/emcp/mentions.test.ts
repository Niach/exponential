// @vitest-environment node
import { describe, expect, it, vi } from "vitest"
import { z } from "zod"
import { Client } from "@modelcontextprotocol/sdk/client/index.js"
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js"

// #NEW-141: the OpenAI MCP extensions over a real MCP connection: the recorded
// per-operation tools are stand-ins, everything else is the server's own.
vi.mock(`@/lib/runtime-config`, () => ({ buildRuntimeConfig: () => ({}) }))
vi.mock(`./tools`, () => ({
  registerExponentialTools: (server: { registerTool: (name: string, def: unknown, handler: unknown) => void }) => {
    const rows = [{ id: `u1`, identifier: `EXP-7`, title: `Board keys`, status: `backlog`, priority: `none` }]
    const answer = (value: unknown, isError = false) => ({ isError, content: [{ type: `text`, text: typeof value === `string` ? value : JSON.stringify(value) }] })
    server.registerTool(`exponential_issues_list`, { annotations: { readOnlyHint: true }, inputSchema: z.strictObject({ search: z.string().min(1).optional(), sort: z.string(), limit: z.number() }) }, async (args: { search?: string }) => answer(args.search === `none` ? [] : rows))
    server.registerTool(`exponential_issues_show`, { annotations: { readOnlyHint: true }, inputSchema: z.strictObject({ limit: z.number().default(50), offset: z.number().default(0) }) }, async () => answer(rows))
    server.registerTool(`exponential_issues_get`, { annotations: { readOnlyHint: true }, inputSchema: z.strictObject({ id: z.string() }) }, async ({ id }: { id: string }) => (id === `EXP-7` ? answer({ identifier: id, title: `Board keys` }) : answer(`Issue not found`, true)))
  },
}))
const { createExponentialMcpServer } = await import(`./server`)
const { FULL_ACCESS } = await import(`./scope`)

async function connected(sessionId: string | null) {
  const server = createExponentialMcpServer({ id: `u` } as never, new Request(`https://x.test/api/mcp`), FULL_ACCESS, sessionId)
  const client = new Client({ name: `host`, version: `1` })
  const [a, b] = InMemoryTransport.createLinkedPair()
  await Promise.all([server.connect(a), client.connect(b)])
  return client
}

type Call = { isError?: boolean; content: { text: string }[]; structuredContent?: { items: Record<string, unknown>[] } }

describe(`OpenAI MCP extensions (#NEW-141)`, () => {
  it(`an entrypoint opens the issues view with {}`, async () => {
    const client = await connected(null)
    const opened = (await client.callTool({ name: `exponential_issues`, arguments: {} })) as Call
    expect(opened.isError).toBeFalsy()
    expect(opened.structuredContent?.items).toMatchObject([{ identifier: `EXP-7` }])
    const { tools } = await client.listTools()
    expect(tools.find((tool) => tool.name === `exponential_issues`)?.inputSchema.required ?? []).toEqual([])
    // Every other noun still names its op.
    expect(tools.find((tool) => tool.name === `exponential_devices`)?.inputSchema.required).toEqual([`op`])
  })

  it(`names the mention search, which answers resource links`, async () => {
    const client = await connected(null)
    const { searchTool } = client.getServerCapabilities()?.extensions?.[`openai/mentions`] as { searchTool: string }
    const { tools } = await client.listTools()
    expect(tools.find((tool) => tool.name === searchTool)?.annotations?.readOnlyHint).toBe(true)
    for (const query of [``, `board`]) {
      const found = (await client.callTool({ name: searchTool, arguments: { query } })) as Call
      expect(found.structuredContent?.items).toEqual([{ type: `resource_link`, uri: `exponential://issues/EXP-7`, name: `EXP-7 Board keys`, mimeType: `application/json` }])
    }
    expect(((await client.callTool({ name: searchTool, arguments: { query: `none` } })) as Call).structuredContent).toEqual({ items: [] })
  })

  it(`reads a mentioned issue through the get op`, async () => {
    const client = await connected(null)
    const { contents } = await client.readResource({ uri: `exponential://issues/EXP-7` })
    expect(contents[0]).toMatchObject({ uri: `exponential://issues/EXP-7`, mimeType: `application/json`, text: `{"identifier":"EXP-7","title":"Board keys"}` })
    await expect(client.readResource({ uri: `exponential://issues/EXP-404` })).rejects.toThrow(`Issue not found`)
  })

  it(`a session gets none of it`, async () => {
    const client = await connected(`5b0c2f7e-0000-4000-8000-000000000001`)
    expect(client.getServerCapabilities()?.extensions).toBeUndefined()
    const call = async () => (await client.callTool({ name: `exponential_issues`, arguments: {} })) as Call
    expect(await call().then((r) => r.isError, () => true)).toBe(true)
    await expect(client.readResource({ uri: `exponential://issues/EXP-7` })).rejects.toThrow()
  })
})
