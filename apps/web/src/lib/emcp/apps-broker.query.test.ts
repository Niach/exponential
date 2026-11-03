// @vitest-environment node
// #VAPP-216 (Q4): an app tool's records list runs the FULL structured query
// through the same eUI evaluator as the web host. The shared fixture's JSON
// query cases (`@exponential_app/eui` fixtures/records-query.json
// `valueCases`) replay through the broker path: a `run.records` list
// carrying the case's query, called over MCP, the server read (read policy +
// RLS, covered by the real-db suites) handing the case's rows over.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { Client } from "@modelcontextprotocol/sdk/client/index.js"
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js"
import fixture from "@exponential_app/eui/fixtures/records-query.json" with { type: "json" }
import { withComputed, type RecordRefTarget, type RecordRow } from "@exponential_app/eui/host/query"
import type { Expose } from "@exp/emcp/exposes"

const read = vi.hoisted(() => ({ rows: [] as unknown[], computed: undefined as Record<string, string> | undefined, names: {} as Record<string, string>, refs: undefined as Record<string, unknown> | undefined, calls: [] as { kind: string; contains: Record<string, unknown>; userId: string }[], truncated: false, today: undefined as string | undefined }))
vi.mock(`@/lib/apps/records-service`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/apps/records-service")>()),
  readAppRecordRows: async (_app: unknown, kind: string, contains: Record<string, unknown>, userId: string) => {
    read.calls.push({ kind, contains, userId })
    // VAPP-214: the real read writes the kind's computed fields into the rows.
    return { rows: [...withComputed((read.rows as RecordRow[]).filter((r) => r.kind === kind), kind, read.computed, new Date().toISOString())], truncated: read.truncated }
  },
  teamMemberNames: async () => read.names,
  teamToday: async () => read.today,
  // VAPP-219: the lookups' DB reads are the real-db suite's; here the kind's refs only.
  readRefLookups: async () => ({ refs: read.refs, targets: [], issues: {} }),
}))

const { createAppsMcpServer } = await import(`@/lib/emcp/apps-broker`)
type BrokerTool = import("@/lib/emcp/apps-broker").BrokerTool

const app = { id: `a1`, slug: `timesheet`, name: `Timesheet`, icon: null, color: null, teamId: `t1`, grantedTools: [], upstreams: [], level: `write` as const }

function listTool(records: Record<string, unknown>, input: Expose[`input`] = { type: `object`, properties: {} }, queries?: Record<string, unknown>): BrokerTool {
  const expose = { name: `report`, title: `Report`, description: `A report.`, input, access: `read`, run: { records: { op: `list`, kind: `entry`, ...records } } } as Expose
  return { name: `timesheet__report`, app: { ...app, queries }, expose }
}

async function call(tool: BrokerTool, me: string, args: Record<string, unknown> = {}) {
  const server = createAppsMcpServer({ user: { id: me } as never, request: new Request(`http://x/api/mcp/apps`), teamId: `t1`, teamSlug: `acme` }, [tool])
  const [clientSide, serverSide] = InMemoryTransport.createLinkedPair()
  const client = new Client({ name: `test`, version: `1.0.0` })
  await server.connect(serverSide)
  await client.connect(clientSide)
  const result = await client.callTool({ name: tool.name, arguments: args })
  await client.close()
  return result as { isError?: boolean; structuredContent?: Record<string, unknown>; content: { text: string }[]; _meta?: Record<string, unknown> }
}

type ValueCase = { name: string; me: string; now: string; names: Record<string, string>; rows: unknown[]; computed?: Record<string, string>; query?: Record<string, unknown>; expected: unknown }
const jsonCases = (fixture.valueCases as unknown as ValueCase[]).filter((c) => c.query && c.query.from === `entry` && c.expected !== null)

beforeEach(() => {
  read.calls = []
  read.truncated = false
  read.computed = undefined
  read.today = undefined
  read.refs = undefined
})
afterEach(() => vi.useRealTimers())

describe(`the broker's records list = the Q1 query (VAPP-216)`, () => {
  it(`replays enough fixture cases to matter`, () => {
    expect(jsonCases.length).toBeGreaterThanOrEqual(12)
  })

  it(`reads the month view's today + default month in the team zone (VAPP-224)`, async () => {
    vi.useFakeTimers({ toFake: [`Date`] })
    vi.setSystemTime(new Date(`2026-09-30T23:30:00Z`))
    read.rows = [{ id: `e1`, kind: `entry`, data: { date: `2026-10-01`, hours: 2 }, createdBy: `u1`, createdAt: `2026-09-30T23:00:00Z`, updatedAt: `2026-09-30T23:00:00Z` }]
    read.today = `2026-10-01`
    const zoned = await call(listTool({ month: { field: `date` } }), `u1`)
    expect((zoned.structuredContent?.month as { key: string; today: string })).toMatchObject({ key: `2026-10`, today: `2026-10-01` })
    expect(zoned.structuredContent?.count).toBe(1)
    read.today = undefined
    const utc = await call(listTool({ month: { field: `date` } }), `u1`)
    expect((utc.structuredContent?.month as { key: string; today: string })).toMatchObject({ key: `2026-09`, today: `2026-09-30` })
  })

  it.each(jsonCases.map((c) => [c.name, c] as const))(`%s`, async (_, c) => {
    vi.useFakeTimers({ toFake: [`Date`] })
    vi.setSystemTime(new Date(c.now))
    read.rows = c.rows
    read.names = c.names
    read.computed = c.computed
    const { from: _from, ...query } = c.query!
    const result = await call(listTool(query), c.me)
    expect(result.isError, result.content[0]?.text).toBeFalsy()
    expect(result.structuredContent).toEqual(c.expected)
    expect(result._meta).toMatchObject({ "at.exponential/app": `timesheet`, "at.exponential/tool": `report`, "at.exponential/via": `records` })
    expect(read.calls).toEqual([{ kind: `entry`, contains: {}, userId: c.me }])
  })

  const base = (jsonCases.find((c) => c.query!.group) ?? jsonCases[0])!

  it(`a named package query, its templates filled from the input; the run's own keys win`, async () => {
    vi.useFakeTimers({ toFake: [`Date`] })
    vi.setSystemTime(new Date(base.now))
    read.rows = base.rows
    read.names = base.names
    const queries = { month: { from: `entry`, where: [[`date`, `between`, `\${start}`, `\${end}`]], group: [`author`], aggregate: { h: [`sum`, `hours`] } } }
    const tool = listTool({ query: `month`, limit: `\${input.limit}` }, { type: `object`, properties: { start: { type: `string` }, end: { type: `string` }, limit: { type: `number` } } }, queries)
    const result = await call(tool, base.me, { start: `2026-10-01`, end: `2026-10-31`, limit: 1 })
    const { parseRecordsJson, recordsValue } = await import(`@exponential_app/eui/host/query`)
    const { query } = parseRecordsJson({ ...queries.month, where: [[`date`, `between`, `2026-10-01`, `2026-10-31`]], limit: 1 })
    expect(result.structuredContent).toEqual(recordsValue(base.rows as RecordRow[], query, { me: base.me, names: base.names }))
    expect((result.structuredContent!.rows as unknown[]).length).toBeLessThanOrEqual(1)
  })

  it(`an object where stays the server's containment prefilter; a list where is the query's`, async () => {
    read.rows = base.rows
    const tool = listTool({ where: { status: `\${input.status}` }, sort: [[`hours`, `asc`]] }, { type: `object`, properties: { status: { type: `string` } } })
    const result = await call(tool, `u1`, { status: `open` })
    expect(result.isError).toBeFalsy()
    expect(read.calls[0]!.contains).toEqual({ status: `open` })
  })

  it(`a capped read says truncated`, async () => {
    read.rows = base.rows
    read.truncated = true
    expect((await call(listTool({}), `u1`)).structuredContent).toMatchObject({ truncated: true })
  })

  it(`another app, another kind or a broken query is refused before any value`, async () => {
    read.rows = base.rows
    for (const [records, words] of [
      [{ app: `crm` }, /^invalidArguments: the query is invalid: \/app: /],
      [{ from: `todo` }, /^invalidArguments: the query is invalid: \/from: /],
      [{ limit: `\${input.n}` }, /^invalidArguments: the query is invalid: \/limit: /],
      [{ query: `nope` }, /^toolError: the app has no query "nope"/],
    ] as const) {
      const result = await call(listTool(records, { type: `object`, properties: { n: { type: `string` } } }), `u1`, { n: `many` })
      expect(result.isError).toBe(true)
      expect(result.content[0]!.text).toMatch(words)
    }
  })
})

// VAPP-219 (R5): the fixture's expand cases through the server's pure half
// (the lookups a real read hands over: the kind's refs, the targets of
// another kind, the issue summaries); a non-ref expand is refused.
describe(`the server query's expand = the R5 evaluator (VAPP-219)`, () => {
  type RefCase = ValueCase & { kind: string; refs?: Record<string, RecordRefTarget>; issues?: Record<string, Record<string, unknown>> }
  const refCases = (fixture.valueCases as unknown as RefCase[]).filter((c) => c.refs && ((c.query as { expand?: string[] } | undefined)?.expand ?? []).length && (c.query as { expand: string[] }).expand.every((f) => f in c.refs!))
  it(`replays the JSON expand cases`, async () => {
    expect(refCases.length).toBeGreaterThanOrEqual(1)
    const { serverRecordsValue } = await import(`@/lib/apps/records-service`)
    for (const c of refCases) {
      const own = (c.rows as RecordRow[]).filter((r) => r.kind === c.kind)
      const targets = (c.rows as RecordRow[]).filter((r) => r.kind !== c.kind)
      const { from: _from, ...query } = c.query!
      const out = serverRecordsValue(own, c.kind, query, { me: c.me, names: c.names, now: c.now, refs: c.refs, targets, issues: c.issues ?? {} })
      expect(out.value, c.name).toEqual(c.expected)
    }
  })
  it(`an expand that is not a ref of the kind is refused`, async () => {
    const { serverRecordsValue } = await import(`@/lib/apps/records-service`)
    const out = serverRecordsValue([], `deal`, { expand: [`title`] }, { me: `u1`, names: {}, refs: { customer: { kind: `company` } } })
    expect(out.issues).toEqual([{ path: `/expand/0`, message: `title is not a ref of deal` }])
  })
})
