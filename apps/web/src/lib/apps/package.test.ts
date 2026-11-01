import { describe, expect, it } from "vitest"
import { APP_SOURCE_NAMES, appCatalogText, upstreamIssues, validateAppPackage } from "@/lib/apps/package"
import emcp from "@exp/emcp/fixtures/exposes.json" with { type: "json" }

// VAPP-110 / VAPP-107: what a stored app package may be.

const pkg = (over: Record<string, unknown> = {}) => ({
  id: `acme.board`,
  name: `Board`,
  version: `1.0.0`,
  catalogId: `https://ui.exponential.at/catalogs/exponential-app/v1`,
  sources: [`exp:issues`],
  templates: {
    main: {
      bindings: [{ path: `/issues`, source: `exp:issues?board=b1&limit=5` }],
      components: [{ id: `root`, component: `Text`, text: { path: `/issues/count` } }],
    },
  },
  ...over,
})

describe(`validateAppPackage`, () => {
  it(`accepts a package that declares the sources it binds`, () => {
    expect(validateAppPackage(pkg()).id).toBe(`acme.board`)
  })

  it(`refuses a package without a sources list`, () => {
    const { sources: _omit, ...rest } = pkg()
    expect(() => validateAppPackage(rest)).toThrow(/must declare sources/)
  })

  it(`refuses a binding to a source the package did not declare`, () => {
    expect(() => validateAppPackage(pkg({ sources: [`exp:devices`] }))).toThrow(/not in the package's sources/)
  })

  it(`refuses a source no app host resolves`, () => {
    expect(() => validateAppPackage(pkg({ sources: [`exp:issues`, `exp:secrets`] }))).toThrow(/exp:secrets/)
  })

  it(`names the host sources and the sources rule in the catalog`, () => {
    const text = appCatalogText()
    for (const name of APP_SOURCE_NAMES) expect(text).toContain(name)
    expect(text).toContain(`sources: [`)
  })
})

// VAPP-161: the package format version 2 on Exponential's hosts.
describe(`validateAppPackage: format version 2`, () => {
  const records = { entry: { schema: { type: `object`, properties: { hours: { type: `number` } } } } }
  const neutral = (over: Record<string, unknown> = {}) =>
    pkg({
      formatVersion: 2,
      sources: [`records:*`],
      records,
      functions: [`records.*`],
      templates: { main: { bindings: [{ path: `/entries`, source: `records:entry?mine=1` }], components: [{ id: `root`, component: `Text`, text: { path: `/entries/count` } }] } },
      ...over,
    })

  it(`accepts the neutral records spelling`, () => {
    expect(validateAppPackage(neutral()).id).toBe(`acme.board`)
    expect(validateAppPackage(neutral({ sources: [`records:entry`] })).id).toBe(`acme.board`)
  })

  it(`refuses a records binding to an undeclared kind`, () => {
    expect(() => validateAppPackage(neutral({ records: {} }))).toThrow(/records:entry names no declared kind/)
  })

  it(`refuses tools: Exponential's app hosts run none`, () => {
    expect(() => validateAppPackage(neutral({ tools: { weather: { kind: `mcp`, title: `Weather`, url: `https://mcp.example.com` } } }))).toThrow(/run no package tools/)
  })

  it(`checks extensions against the app extension`, () => {
    const id = `https://ui.exponential.at/catalogs/exponential-app/v1`
    expect(validateAppPackage(neutral({ extensions: [{ id, version: `1.0.0`, components: [`IssueRow`] }] })).id).toBe(`acme.board`)
    expect(() => validateAppPackage(neutral({ extensions: [{ id, version: `1.0.0`, components: [`Nope`] }] }))).toThrow(/has no component Nope/)
    expect(() => validateAppPackage(neutral({ extensions: [{ id: `https://acme.example/x/v1`, version: `1.0.0` }] }))).toThrow(/not available on this host/)
  })

  it(`refuses a newer format version`, () => {
    expect(() => validateAppPackage(neutral({ formatVersion: 3 }))).toThrow(/format version 3 is newer than this host/)
  })

  it(`names the neutral spelling in the catalog`, () => {
    const text = appCatalogText()
    expect(text).toContain(`records:<kind>`)
    expect(text).toContain(`records.upsert`)
  })
})

// VAPP-180: an app's exposed MCP tools: the SDK checks the format, the host
// adds eMCP's action rules (offered, grantable, not destructive by its args)
// and the team's MCP servers.
describe(`validateAppPackage: exposes`, () => {
  const records = { entry: { schema: { type: `object`, properties: { hours: { type: `number` } } } } }
  const withExposes = (exposes: unknown[], upstreams: string[] = [`m5`]) => pkg({ records, upstreams, exposes })

  it.each(emcp.validateCases)(`$name`, (c) => {
    const records = Object.fromEntries(c.context.kinds.map((k) => [k, { schema: { type: `object`, properties: {} } }]))
    const run = () => validateAppPackage(pkg({ records, upstreams: c.context.upstreams, exposes: c.exposes }))
    if (c.expected.length === 0) expect(run().exposes).toHaveLength(c.exposes.length)
    else expect(run).toThrow(/exposes|upstream/)
  })

  it(`refuses an action that is not an offered operation, owner-only or destructive by its args`, () => {
    const one = (run: unknown) => [{ name: `t`, title: `T`, description: `T.`, input: { type: `object`, properties: { s: { type: `string` } } }, access: `write`, run }]
    expect(() => validateAppPackage(withExposes(one({ action: `harness.toast`, args: {} })))).toThrow(/exponential.<noun>.<op> operations/)
    expect(() => validateAppPackage(withExposes(one({ action: `exponential.people.invites_create`, args: {} })))).toThrow(/owner-only/)
    expect(() => validateAppPackage(withExposes(one({ action: `exponential.sessions.send`, args: { sessionId: `\${input.s}`, stop: true } })))).toThrow(/destructive call/)
    expect(validateAppPackage(withExposes(one({ action: `exponential.sessions.send`, args: { sessionId: `\${input.s}`, text: `hi` } }))).exposes).toHaveLength(1)
  })

  it(`upstreams name MCP servers of the app's team`, () => {
    const valid = validateAppPackage(withExposes([], [`m5`, `github`]))
    expect(upstreamIssues(valid, [`m5`, `github`])).toEqual([])
    expect(upstreamIssues(valid, [`m5`])).toEqual([{ path: `/upstreams`, message: `upstreams github are not MCP servers of this team: the team has m5` }])
    expect(upstreamIssues(valid, [])[0]!.message).toMatch(/no MCP servers yet/)
  })

  it(`documents exposes in the catalog`, () => {
    const text = appCatalogText()
    expect(text).toContain(`# Exposed MCP tools`)
    for (const word of [`records:`, `action:`, `upstream:`, `upstreams:`, `\${input.`]) expect(text).toContain(word)
  })
})
