import { describe, expect, it } from "vitest"
import { APP_SOURCE_NAMES, appCatalogText, validateAppPackage } from "@/lib/apps/package"

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
