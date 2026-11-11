import { cleanup, render, screen, waitFor } from "@testing-library/react"
import standup from "../../../../samples/apps/standup.app.json"
import { AppRun, consentCopy, instanceOrigin } from "./genui-app"
import { createConsentGate, createMcpAppHost, teamPathOf, type CallTool } from "./mcp-host"
import { decodeGenuiResult, inlineShowResult, type GenuiAppRead } from "./result"

afterEach(cleanup)

const text = (data: unknown) => ({ content: [{ type: `text`, text: JSON.stringify(data) }] })

const TEAM = `00000000-0000-4000-8000-000000000001`
const ME = `user-me`

function standupApp(): GenuiAppRead {
  return {
    id: `00000000-0000-4000-8000-0000000000aa`,
    slug: `standup`,
    name: `Standup`,
    teamId: TEAM,
    url: `https://app.exponential.at/t/acme/apps/standup`,
    version: 1,
    package: standup.package as unknown as Record<string, unknown>,
    grantedTools: [],
    viewer: { id: ME, access: `write` },
  }
}

/** Exponential's /api/mcp as the MCP Apps host forwards it, over a store. */
function fakeServer(today: string) {
  const records = [
    { id: `00000000-0000-4000-8000-0000000000r1`, kind: `update`, data: { date: today, yesterday: `Shipped the connector`, today: `Write the guide` }, createdBy: ME, authorId: ME, createdAt: `${today}T08:00:00.000Z`, updatedAt: `${today}T08:00:00.000Z` },
  ]
  const calls: Array<{ name: string; args: Record<string, unknown> }> = []
  const call: CallTool = async (name, args) => {
    calls.push({ name, args })
    if (name === `exponential_apps` && args.op === `records_list`) return text({ records })
    if (name === `exponential_apps` && args.op === `records_upsert`) {
      const id = `00000000-0000-4000-8000-0000000000r${records.length + 1}`
      records.push({ id, kind: String(args.kind), data: args.data as never, createdBy: ME, authorId: ME, createdAt: `${today}T09:00:00.000Z`, updatedAt: `${today}T09:00:00.000Z` })
      return text({ ok: true, id, created: true })
    }
    if (name === `exponential_people`) return text([{ id: ME, name: `Danny`, email: `danny@exponential.at`, image: null, role: `owner` }])
    if (name === `exponential_teams`) return text({ id: TEAM, timeZone: `UTC` })
    return text([])
  }
  return { call, calls, records }
}

describe(`decodeGenuiResult`, () => {
  it(`tells an inline card, an app, the apps and anything else apart`, () => {
    const card = { kind: `surface` as const, surface: { root: { id: `s`, component: `Stack` } }, title: null }
    expect(decodeGenuiResult(text(inlineShowResult(card)))).toEqual({ kind: `card`, card })
    expect(decodeGenuiResult(text(standupApp())).kind).toBe(`app`)
    expect(decodeGenuiResult(text([{ id: `a`, slug: `crm`, name: `CRM`, teamId: TEAM }])).kind).toBe(`apps`)
    expect(decodeGenuiResult(text({ records: [] }))).toEqual({ kind: `none` })
    expect(decodeGenuiResult({ isError: true, content: [{ type: `text`, text: `Not a member` }] })).toEqual({ kind: `error`, message: `Not a member` })
  })
})

describe(`the MCP app host`, () => {
  it(`writes a record through exponential_apps and re-reads every binding; an undeclared call never runs`, async () => {
    const server = fakeServer(`2026-11-11`)
    const consent = createConsentGate()
    const host = createMcpAppHost({ app: standupApp(), call: server.call, origin: `https://app.exponential.at`, consent, openLink: () => {}, notify: () => {} })
    host.applyTemplate({ surfaceId: `s`, templateId: Object.keys((standup.package as { templates: Record<string, unknown> }).templates)[0]!, packageId: standup.package.id })
    const reads = () => server.calls.filter((c) => c.args.op === `records_list`).length
    await waitFor(() => expect(reads()).toBeGreaterThan(0))
    const before = reads()
    const out = await host.callFunction({ surfaceId: `s`, componentId: `x`, name: `records_upsert`, args: { kind: `update`, data: { date: `2026-11-11`, yesterday: `a`, today: `b` } } })
    expect(out.decision).toBe(`allow`)
    expect(server.calls.find((c) => c.args.op === `records_upsert`)?.args).toMatchObject({ slug: `standup`, teamId: TEAM, kind: `update` })
    await waitFor(() => expect(reads()).toBeGreaterThan(before))
    // A call the package does not declare never reaches a tool.
    const refused = await host.callFunction({ surfaceId: `s`, componentId: `x`, name: `harness_mcp`, args: { tool: `exponential_issues`, arguments: { op: `delete`, id: `EXP-1` } } })
    expect(refused.decision).not.toBe(`allow`)
    expect(consent.current()).toBeNull()
    expect(server.calls.some((c) => c.name === `exponential_issues`)).toBe(false)
    host.close()
  })

  it(`resolves ~/ against the app's team`, () => {
    expect(teamPathOf(`https://app.exponential.at/t/acme/apps/standup`)).toBe(`/t/acme`)
    expect(teamPathOf(null)).toBeNull()
  })

  it(`words consent like web (destructive = the destructive Allow)`, () => {
    const plain = consentCopy(`harness_mcp`, { tool: `exponential_issues`, arguments: { op: `list` } })
    expect(plain.title).toBe(`Allow this surface to run exponential_issues?`)
    expect(plain.actions.find((a) => a.id === `allow`)?.role).toBe(`default`)
    const destructive = consentCopy(`harness_mcp`, { tool: `exponential_issues`, arguments: { op: `delete`, id: `EXP-1` } })
    expect(destructive.actions.find((a) => a.id === `allow`)?.role).toBe(`destructive`)
    expect(destructive.args).toBe(`{"op":"delete","id":"EXP-1"}`)
  })

  it(`reads the instance origin the server stamped`, () => {
    const doc = document.implementation.createHTMLDocument()
    doc.head.innerHTML = `<meta name="exp-origin" content="https://exp.example.com" />`
    expect(instanceOrigin(doc)).toBe(`https://exp.example.com`)
  })
})

describe(`AppRun`, () => {
  it(`runs a team app over tools/call: its records paint`, async () => {
    const today = new Date().toISOString().slice(0, 10)
    const server = fakeServer(today)
    render(<AppRun app={standupApp()} call={server.call} origin="https://app.exponential.at" openLink={() => {}} phone={false} />)
    await waitFor(() => expect(screen.getAllByText(/Write the guide/).length).toBeGreaterThan(0), { timeout: 5000 })
    expect(server.calls.some((c) => c.name === `exponential_apps` && c.args.op === `records_list` && c.args.slug === `standup` && c.args.teamId === TEAM)).toBe(true)
  })
})
