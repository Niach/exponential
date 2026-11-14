import { describe, expect, it } from "vitest"
import { APP_CATALOG_SECTIONS, APP_SOURCE_NAMES, appCatalogText, appPackageIssues, linkableScreens, upstreamIssues, validateAppPackage } from "@/lib/apps/package"
import emcp from "@exp/emcp/fixtures/exposes.json" with { type: "json" }

// VAPP-110 / VAPP-107: what a stored app package may be.

const pkg = (over: Record<string, unknown> = {}) => ({
  formatVersion: 5,
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

  it(`#VAPP-230: an upstream binding names a declared upstream`, () => {
    const up = (over: Record<string, unknown>) =>
      pkg({ sources: [`upstream:crm/*`], templates: { main: { bindings: [{ path: `/mrr`, source: `upstream:crm/mrr?months=3` }], components: [{ id: `root`, component: `Text`, text: { path: `/mrr/total` } }] } }, ...over })
    expect(validateAppPackage(up({ upstreams: [`crm`] })).id).toBe(`acme.board`)
    expect(validateAppPackage(up({ upstreams: [`crm`], sources: [`upstream:*`] })).id).toBe(`acme.board`)
    expect(() => validateAppPackage(up({ upstreams: [] }))).toThrow(/does not declare/)
    expect(() => validateAppPackage(up({ upstreams: [`crm`], sources: [`upstream:crm/*`, `upstream:crm`] }))).toThrow(/not an upstream:<server>\/<tool> source/)
  })

  it(`refuses a source no app host resolves`, () => {
    expect(() => validateAppPackage(pkg({ sources: [`exp:issues`, `exp:secrets`] }))).toThrow(/exp:secrets/)
    expect(() => validateAppPackage(pkg({ sources: [`exp:issues`, `exp:records`] }))).toThrow(/exp:records/)
  })

  it(`refuses a package without format version 5`, () => {
    const { formatVersion: _omit, ...rest } = pkg()
    expect(() => validateAppPackage(rest)).toThrow(/formatVersion is required \(6\)/)
    expect(() => validateAppPackage(pkg({ formatVersion: 1 }))).toThrow(/format version 1 is no longer read; 6 is required/)
  })

  it(`names the host sources and the sources rule in the catalog`, () => {
    const text = appCatalogText()
    for (const name of APP_SOURCE_NAMES) expect(text).toContain(name)
    expect(text).toContain(`sources: [`)
  })
})

// VAPP-161: the package format version 5 on Exponential's hosts.
describe(`validateAppPackage: format version 5`, () => {
  const records = { entry: { schema: { type: `object`, properties: { hours: { type: `number` } } } } }
  const neutral = (over: Record<string, unknown> = {}) =>
    pkg({
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
    expect(() => validateAppPackage(neutral({ formatVersion: 7 }))).toThrow(/format version 7 is newer than this host/)
  })

  it(`stores no format 6 package until the flag day (a released client reads format 5 alone)`, () => {
    expect(() => validateAppPackage(pkg({ formatVersion: 6 }))).toThrow(/format 6 is not stored yet: write formatVersion 5/)
  })

  it(`names the neutral spelling in the catalog`, () => {
    const text = appCatalogText()
    expect(text).toContain(`records:<kind>`)
    expect(text).toContain(`records.upsert`)
    expect(text).toContain(`formatVersion: 5`)
    // Round 9 §14: the names a format 5 package writes.
    expect(text).toContain(`issues.update_status,`)
    expect(text).not.toContain(`issues_update_status`)
    expect(text).not.toContain(`exp:records`)
    expect(text).not.toContain(`harness.records`)
  })
})

// VAPP-180: an app's exposed MCP tools: the SDK checks the format, the host
// adds eMCP's action rules (offered, grantable, not destructive by its args)
// and the team's MCP servers.
describe(`validateAppPackage: exposes`, () => {
  const records = { entry: { schema: { type: `object`, properties: { hours: { type: `number` } } } } }
  const withExposes = (exposes: unknown[], upstreams: string[] = [`crm`]) => pkg({ records, upstreams, exposes })

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
    const valid = validateAppPackage(withExposes([], [`crm`, `github`]))
    expect(upstreamIssues(valid, [`crm`, `github`])).toEqual([])
    expect(upstreamIssues(valid, [`crm`])).toEqual([{ path: `/upstreams`, message: `upstreams github are not MCP servers of this team: the team has crm` }])
    expect(upstreamIssues(valid, [])[0]!.message).toMatch(/no MCP servers yet/)
  })

  it(`documents exposes in the catalog`, () => {
    const text = appCatalogText()
    expect(text).toContain(`# Exposed MCP tools`)
    for (const word of [`records:`, `action:`, `upstream:`, `upstreams:`, `\${input.`]) expect(text).toContain(word)
  })
})

describe(`validateAppPackage: automations (#VAPP-220)`, () => {
  const records = { request: { schema: { type: `object`, properties: { title: { type: `string` } } } } }
  const one = (over: Record<string, unknown> = {}) => [{ name: `notify`, on: { record: { kind: `request`, op: `create` } }, run: { records: { op: `upsert`, kind: `request`, data: { title: `\${record.title}` } } }, ...over }]

  it(`accepts a valid automation`, () => {
    expect((validateAppPackage(pkg({ records, automations: one() })) as { automations?: unknown[] }).automations).toHaveLength(1)
  })

  it(`refuses each automations.json code with words`, () => {
    expect(() => validateAppPackage(pkg({ records, automations: one({ on: { schedule: { cron: `* * * * *` } } }) }))).toThrow(/at most every 15 minutes/)
    expect(() => validateAppPackage(pkg({ records, automations: one({ run: { action: `exponential.apps.records_upsert`, args: {} } }) }))).toThrow(/never reaches an app/)
    expect(() => validateAppPackage(pkg({ records, automations: one({ run: { action: `exponential.issues.delete`, args: {} } }) }))).toThrow(/destructive/)
    expect(() => validateAppPackage(pkg({ records, automations: one({ run: { records: { op: `upsert`, kind: `request`, data: { t: `\${event.x}` } } } }) }))).toThrow(/templates/)
  })

  it(`a schedule's cron parses as the watchdog reads it (names included)`, () => {
    expect(() => validateAppPackage(pkg({ records, automations: one({ on: { schedule: { cron: `0 8 * * FOO` } }, run: { records: { op: `upsert`, kind: `request`, data: { title: `x` } } } }) }))).toThrow(/schedule is/)
    expect(() => validateAppPackage(pkg({ records, automations: one({ on: { schedule: { cron: `0 8 * * MON-FRI` } }, run: { records: { op: `upsert`, kind: `request`, data: { title: `x` } } } }) }))).not.toThrow()
  })

  it(`documents automations in the catalog`, () => {
    const text = appCatalogText()
    expect(text).toContain(`# Automations`)
    for (const word of [`\${record.`, `\${event.`, `\${firedAt}`, `sponsor`]) expect(text).toContain(word)
  })
})

// #FEED-201: how a screen is declared and drawn, in the text agents read.
it(`documents the screen manifest in the catalog`, () => {
  const text = appCatalogText()
  expect(text).toContain(`# Screens`)
  for (const word of [`screens: [{template, title`, `nav?: false`, `exposed?:`, `widget?:`, `Array order`, `≤ 5`, `sidebar rows`, `bottom tab bar`]) expect(text).toContain(word)
})

// #EXP-1674: `apps get`'s link list and the reserved query keys.
describe(`linkableScreens`, () => {
  const screens = [
    { template: `home`, title: `Home` },
    { template: `deal`, title: `Deal`, nav: false, exposed: { description: `A deal`, params: { id: { required: true, description: `Its id` }, tab: {} } } },
    { template: `hidden`, title: `Hidden`, nav: false },
  ]

  it(`lists navigation and exposed screens with a ready link`, () => {
    expect(linkableScreens(`my crm`, { screens })).toEqual([
      { screen: `home`, title: `Home`, nav: true, exposed: false, params: {}, link: `~/apps/my%20crm/home` },
      { screen: `deal`, title: `Deal`, nav: false, exposed: true, description: `A deal`, params: { id: { required: true, description: `Its id` }, tab: { required: false } }, link: `~/apps/my%20crm/deal?id=<id>` },
    ])
    expect(linkableScreens(`x`, null)).toEqual([])
  })

  it(`refuses a contract naming a key a link's query never carries`, () => {
    const template = Object.values(pkg().templates)[0]
    const templates = { home: template, deal: template }
    const reserved = [{ template: `home`, title: `Home` }, { template: `deal`, title: `Deal`, nav: false, exposed: { description: `A deal`, params: { tab: {} } } }]
    expect(appPackageIssues(pkg({ templates, screens: reserved })).map((issue) => issue.path)).toContain(`/screens/1/exposed/params/tab`)
  })
})

describe(`apps catalog sections (#FEED-218)`, () => {
  const full = appCatalogText()
  const parts = APP_CATALOG_SECTIONS.filter((name) => name !== `index`)

  it(`the full text is every section but the index, in order`, () => {
    expect(appCatalogText([...parts])).toBe(full)
    expect(parts.map((name) => appCatalogText([name])).join(`\n\n`)).toBe(full)
  })

  it(`a section is a part of the full text, in the catalog's order whatever the order asked`, () => {
    for (const name of parts) expect(full).toContain(appCatalogText([name]))
    expect(appCatalogText([`sources`, `package`])).toBe(`${appCatalogText([`package`])}\n\n${appCatalogText([`sources`])}`)
  })

  it(`the index lists every component on one line, far smaller than the props`, () => {
    const index = appCatalogText([`index`])
    expect(index).toContain(`Stack (children): `)
    expect(index.length).toBeLessThan(appCatalogText([`components`]).length / 3)
  })

  it(`names the sections an unknown name could have meant`, () => {
    expect(appCatalogText([`nope`])).toBe(`No such section: nope (sections: ${APP_CATALOG_SECTIONS.join(`, `)})`)
  })

  it(`says member ids are user ids`, () => {
    expect(appCatalogText([`records`])).toContain(`USER id (exponential_people id, never its memberId)`)
    expect(appCatalogText([`tests`])).toContain(`as?: a user id`)
  })
})
