import { describe, expect, it } from "vitest"
// Read straight out of the hoisted install (bunfig sets `linker = "hoisted"`),
// so the banner is pinned to what the packages we actually ship really say —
// a version bump that changes a copyright line fails here, loudly.
import preactLicense from "../../../node_modules/preact/LICENSE?raw"
import snapdomLicense from "../../../node_modules/@zumer/snapdom/LICENSE?raw"
import pkg from "../package.json"
import { legalBanner } from "./legal-banner"

// EXP-377: the emitted bundles are served cross-origin to customer sites, so
// the MIT copyright notices have to ride along. These assertions are the
// banner's contract; that it SURVIVES minification is verified by building and
// grepping apps/web/public/widget/v1/{loader,widget}.js.
describe(`legal banner`, () => {
  it(`is an esbuild legal comment`, () => {
    expect(legalBanner.startsWith(`/*!`)).toBe(true)
    expect(legalBanner.endsWith(`*/`)).toBe(true)
    // A nested `*/` would truncate the banner and comment out the bundle.
    expect(legalBanner.slice(3, -2)).not.toContain(`*/`)
  })

  it(`names every bundled dependency`, () => {
    const deps = Object.keys(pkg.dependencies)
    expect(deps.length).toBeGreaterThan(0)
    for (const name of deps) expect(legalBanner).toContain(name)
  })

  it(`carries each dependency's real copyright line`, () => {
    const copyrightLine = (licence: string) => {
      const line = licence
        .split(/\r?\n/)
        .find((l) => l.startsWith(`Copyright`))
        ?.trim()
      expect(line).toBeTruthy()
      return line as string
    }
    expect(legalBanner).toContain(copyrightLine(preactLicense))
    expect(legalBanner).toContain(copyrightLine(snapdomLicense))
  })

  it(`points at the repo NOTICE, never at an instance`, () => {
    expect(legalBanner).toContain(
      `https://github.com/Niach/exponential/blob/master/NOTICE`
    )
    expect(legalBanner).not.toContain(`exponential.at`)
  })
})
