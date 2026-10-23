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
