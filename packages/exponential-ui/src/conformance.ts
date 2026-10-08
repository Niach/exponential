// VAPP-91: the Exponential UI conformance suite. The fixtures under
// fixtures/ are the contract; conformance/manifest.json (GENERATED, drift-
// gated) names every suite, its files, what a renderer must do with each
// case and how many cases it has. A renderer's runner replays the suites and
// writes a report; `checkReport` decides conformance: every suite present,
// every case run, nothing failed.

import componentsFixture from "../fixtures/catalog-components.json" with { type: "json" }
import macrosFixture from "../fixtures/catalog-macros.json" with { type: "json" }
import basicFixture from "../fixtures/catalog-basic-map.json" with { type: "json" }
import extensionFixture from "../fixtures/catalog-extension.json" with { type: "json" }
import resolvedFixture from "../fixtures/theme-resolved.json" with { type: "json" }
import recipesFixture from "../fixtures/theme-recipes.json" with { type: "json" }
import extendsFixture from "../fixtures/theme-extends.json" with { type: "json" }
import invalidFixture from "../fixtures/theme-invalid.json" with { type: "json" }
import controlFixture from "../fixtures/control-geometry.json" with { type: "json" }
import layoutFixture from "../fixtures/layout-geometry.json" with { type: "json" }
import overlayFixture from "../fixtures/overlay-geometry.json" with { type: "json" }
import transportFixture from "../fixtures/host-transport.json" with { type: "json" }
import policyFixture from "../fixtures/host-policy.json" with { type: "json" }
import routerFixture from "../fixtures/host-router.json" with { type: "json" }
import { BUILTIN_THEME_IDS } from "./themes"
import { MODES } from "./theme"
import { HOST_CONTRACT_VERSION } from "./host/contract"

export const CONFORMANCE_VERSION = 1

export interface ConformanceSuite {
  id: string
  files: string[]
  /** What a runner does with each case. */
  check: string
  /** How a case is counted. */
  unit: string
  cases: number
}

export interface ConformanceManifest {
  $comment: string
  version: number
  hostContractVersion: number
  suites: ConformanceSuite[]
}

const n = (v: unknown) => (Array.isArray(v) ? v.length : Object.keys(v as object).length)

const BUNDLED: Record<string, unknown> = {
  "catalog-components.json": componentsFixture,
  "catalog-macros.json": macrosFixture,
  "catalog-basic-map.json": basicFixture,
  "catalog-extension.json": extensionFixture,
  "theme-resolved.json": resolvedFixture,
  "theme-recipes.json": recipesFixture,
  "theme-extends.json": extendsFixture,
  "theme-invalid.json": invalidFixture,
  "control-geometry.json": controlFixture,
  "layout-geometry.json": layoutFixture,
  "overlay-geometry.json": overlayFixture,
  "host-transport.json": transportFixture,
  "host-policy.json": policyFixture,
  "host-router.json": routerFixture,
}

/** The manifest over the bundled fixtures, or over `fixture(name)` (the
 *  generator passes the files it is about to write). */
export function conformanceManifest(fixture: (name: string) => unknown = (name) => BUNDLED[name]): ConformanceManifest {
  /* eslint-disable @typescript-eslint/no-explicit-any */
  const f = (name: string) => fixture(name) as any
  const componentsFixture = f(`catalog-components.json`), macrosFixture = f(`catalog-macros.json`), basicFixture = f(`catalog-basic-map.json`)
  const extensionFixture = f(`catalog-extension.json`), resolvedFixture = f(`theme-resolved.json`), recipesFixture = f(`theme-recipes.json`)
  const extendsFixture = f(`theme-extends.json`), invalidFixture = f(`theme-invalid.json`), controlFixture = f(`control-geometry.json`)
  const layoutFixture = f(`layout-geometry.json`), overlayFixture = f(`overlay-geometry.json`)
  const transportFixture = f(`host-transport.json`), policyFixture = f(`host-policy.json`), routerFixture = f(`host-router.json`)
  const controlCases = Object.values(controlFixture.themes as Record<string, Record<string, { cases: object }>>).reduce((sum, t) => sum + Object.values(t).reduce((s, c) => s + n(c.cases), 0), 0)
  const suites: ConformanceSuite[] = [
    { id: `catalog`, files: [`fixtures/catalog-components.json`], unit: `case`, cases: n(componentsFixture.cases), check: `Reduce the case's nested node with the core catalog: no issues. Paint it under the default theme: no crash, no Unknown placeholder, every visible node painted.` },
    { id: `macros`, files: [`fixtures/catalog-macros.json`], unit: `case`, cases: n(macrosFixture.cases), check: `Expand the case: the tree equals \`expanded\` (JSON-equal, recipes included).` },
    { id: `basic-map`, files: [`fixtures/catalog-basic-map.json`], unit: `case`, cases: n(basicFixture.cases), check: `Reduce the A2UI basic surface: the tree and the issues equal the expected ones.` },
    { id: `extension`, files: [`fixtures/catalog-extension.json`], unit: `case`, cases: n(extensionFixture.cases), check: `Register the extension; reduce each case: tree + issues equal; its natives reach the extension painter (an \`Extension\` leaf of that kind).` },
    { id: `theme-resolved`, files: [`fixtures/theme-resolved.json`], unit: `theme`, cases: n(resolvedFixture.themes), check: `Load each built-in theme: the resolved theme equals the fixture.` },
    { id: `theme-recipes`, files: [`fixtures/theme-recipes.json`], unit: `case`, cases: n(recipesFixture.cases), check: `Resolve the part's recipe for the case's theme, props, mode and states: the visual equals the fixture.` },
    { id: `theme-extends`, files: [`fixtures/theme-extends.json`], unit: `case`, cases: n(extendsFixture.cases), check: `Load the theme over its parents: the chain and the probed styles equal the fixture.` },
    { id: `theme-invalid`, files: [`fixtures/theme-invalid.json`], unit: `case`, cases: n(invalidFixture.cases), check: `Validate the bad theme: the same issue paths, never a crash.` },
    { id: `control-geometry`, files: [`fixtures/control-geometry.json`], unit: `theme × control × case`, cases: controlCases, check: `Measure the control under the theme: the box (height, padding, gap, border, radius) equals the fixture.` },
    { id: `layout`, files: [`fixtures/layout-geometry.json`], unit: `width × direction`, cases: n(layoutFixture.cases), check: `Lay the surface out with the fixture's fixed measure: every node's frame equals the fixture (exact for the Rust core and native painters, within 1 px for a CSS renderer).` },
    { id: `overlay`, files: [`fixtures/overlay-geometry.json`], unit: `case`, cases: n(overlayFixture.cases), check: `Place the overlay: side, flip and frame equal the fixture within \`tolerancePx\`.` },
    { id: `replay`, files: [`fixtures/kitchen-sink.json`, `fixtures/kitchen-sink.expanded.json`], unit: `theme × mode`, cases: BUILTIN_THEME_IDS.length * MODES.length, check: `Paint the kitchen sink under each built-in theme and mode: the reduced tree equals kitchen-sink.expanded.json, no Unknown, the accessibility order is the pre-order of the painted nodes.` },
    { id: `host-transport`, files: [`fixtures/host-transport.json`], unit: `case`, cases: n(transportFixture.jsonl) + n(transportFixture.sse) + n(transportFixture.mcp) + n(transportFixture.mcpAction), check: `Feed the chunks through ONE decoder (JSONL / SSE), or decode the MCP result, or build the MCP action call: messages + issues equal \`expected\`.` },
    { id: `host-policy`, files: [`fixtures/host-policy.json`], unit: `case`, cases: n(policyFixture.functions) + n(policyFixture.combine) + n(policyFixture.urls) + n(policyFixture.media) + n(policyFixture.sources) + n(policyFixture.negotiation), check: `Decide the function / combine / url, build the media request, parse the source, negotiate the catalog ids: each equals \`expected\`.` },
    { id: `host-router`, files: [`fixtures/host-router.json`], unit: `package or flow`, cases: n(routerFixture.validation) + n(routerFixture.flows), check: `Validate each package; for each flow build ONE router with \`extensionIds\`, install \`packages\` (issues = \`installIssues\`), route every step: the ops equal \`expected\`.` },
  ]
  return {
    $comment: `GENERATED by \`bun run --filter @exponential-at/ui generate\` (VAPP-91). The Exponential UI conformance suite: a renderer is "Exponential UI conformant" when its runner reports every suite below with \`cases\` run and none failed (\`bun run --filter @exponential-at/ui conformance:check <report.json>\`). conformance/README.md says how to write a runner for a new platform.`,
    version: CONFORMANCE_VERSION,
    hostContractVersion: HOST_CONTRACT_VERSION,
    suites,
  }
}

/** What a runner writes (conformance/report.schema.json). */
export interface ConformanceReport {
  renderer: string
  platform: string
  version: string
  conformanceVersion: number
  suites: Record<string, { cases: number; passed: number; failed: string[]; skipped?: string[] }>
}

export interface ReportVerdict {
  conformant: boolean
  problems: string[]
}

export function checkReport(report: ConformanceReport, manifest: ConformanceManifest = conformanceManifest()): ReportVerdict {
  const problems: string[] = []
  if (report.conformanceVersion !== manifest.version) problems.push(`conformanceVersion ${report.conformanceVersion} ≠ ${manifest.version}`)
  for (const suite of manifest.suites) {
    const r = report.suites?.[suite.id]
    if (!r) {
      problems.push(`${suite.id}: missing`)
      continue
    }
    if (r.cases !== suite.cases) problems.push(`${suite.id}: ran ${r.cases} of ${suite.cases} cases`)
    if (r.failed.length) problems.push(`${suite.id}: ${r.failed.length} failed (${r.failed.slice(0, 5).join(`; `)}${r.failed.length > 5 ? `; …` : ``})`)
    if (r.passed + r.failed.length !== r.cases) problems.push(`${suite.id}: passed + failed ≠ cases`)
    if (r.skipped?.length) problems.push(`${suite.id}: ${r.skipped.length} skipped`)
  }
  for (const id of Object.keys(report.suites ?? {})) if (!manifest.suites.some((s) => s.id === id)) problems.push(`${id}: not a suite of this manifest`)
  return { conformant: problems.length === 0, problems }
}
