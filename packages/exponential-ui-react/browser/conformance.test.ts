// The renderer CONFORMANCE gate, web side (packages/exponential-ui/conformance):
// every case of fixtures/conformance-cases.json rendered in headless
// Chromium with the conformance font set must reproduce the committed web
// baseline (fixtures/conformance-baseline.json) node for node within the
// matrix's tolerance — a React layout change shows up here as the nodes it
// moved. Intended changes: `bun run --filter @exponential-at/ui conformance
// -- --write-baseline`, then review the baseline diff. The desktop half
// gates against the same baseline in
// apps/desktop/crates/exponential-ui-gpui/tests/conformance.rs.

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { chromium, type Browser } from "playwright"
import { spawn, type ChildProcess } from "node:child_process"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { allCases, BASELINE_PATH, MANIFEST_PATH, type ConformanceManifest } from "../../exponential-ui/conformance/dump"
import { decodeBaseline, type Baseline } from "../../exponential-ui/conformance/baseline"
import { compareCase, formatCase } from "../../exponential-ui/conformance/compare"
import { CHROMIUM_ARGS, dumpCase } from "../scripts/conformance-drive"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`)
const manifest = JSON.parse(readFileSync(join(repoRoot, MANIFEST_PATH), `utf8`)) as ConformanceManifest
const baseline = decodeBaseline(JSON.parse(readFileSync(join(repoRoot, BASELINE_PATH), `utf8`)) as Baseline)
const cases = allCases(manifest)

let proc: ChildProcess
let browser: Browser
let base = ``

beforeAll(async () => {
  const port = 4800 + Math.floor(Math.random() * 400)
  proc = spawn(`bun`, [join(import.meta.dirname, `..`, `scripts`, `conformance-serve.ts`)], { env: { ...process.env, PORT: String(port) }, stdio: [`ignore`, `pipe`, `pipe`] })
  base = `http://127.0.0.1:${port}`
  const deadline = Date.now() + 30_000
  for (;;) {
    try {
      const r = await fetch(`${base}/dist/page.js`)
      if (r.ok) break
      if (r.status === 500) throw new Error(await r.text())
    } catch (e) {
      if (!String(e).includes(`fetch failed`) && !String(e).includes(`ECONNREFUSED`)) throw e
    }
    if (Date.now() > deadline) throw new Error(`the conformance server did not start`)
    await new Promise((r) => setTimeout(r, 150))
  }
  browser = await chromium.launch({ args: CHROMIUM_ARGS })
})
afterAll(async () => {
  await browser?.close()
  proc?.kill()
})

describe(`conformance baseline`, () => {
  it(`covers exactly the matrix`, () => {
    expect(Object.keys(baseline.cases).sort()).toEqual(cases.map((c) => c.key).sort())
  })

  for (const c of cases) {
    it(`${c.key}: the web dump reproduces the baseline within tolerance`, async () => {
      const dump = await dumpCase(browser, base, c)
      const report = compareCase(c.key, baseline.cases[c.key], dump, manifest.tolerance)
      const clean = report.diffs.length === 0 && report.onlyRef.length === 0 && report.onlyCand.length === 0
      expect(clean, formatCase(report)).toBe(true)
      expect(report.matched).toBeGreaterThan(10)
    })
  }
})
