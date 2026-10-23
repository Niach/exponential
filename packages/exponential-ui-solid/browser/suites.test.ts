// @vitest-environment jsdom
// FEED-128: the Solid renderer's Exponential UI conformance run through the
// SHARED web runner (@exponential_app/ui/conformance/browser-suites.ts): the
// harness (harness/serve.ts), Solid's own logic (template keying, the action
// runner), the report (EXPONENTIAL_UI_CONFORMANCE_REPORT, default
// ../../.conformance/exponential-ui-solid.json), `checkReport` must pass.
// jsdom only so solid-js's client build loads (it delegates events on
// `window` at module load); every painted suite runs in headless Chromium.

import { expect, it } from "vitest"
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { checkReport } from "@exponential_app/ui/conformance"
import { runBrowserSuites } from "../../exponential-ui/conformance/browser-suites"
import { bunHarness } from "../../exponential-ui/conformance/browser-page"
import { itemInstance, runNodeAction, templateItems } from "../src/node-view"
import type { SurfaceContextValue } from "../src/context"

const pkg = JSON.parse(readFileSync(join(import.meta.dirname, `..`, `package.json`), `utf8`)) as { name: string; version: string }

it(`Exponential UI conformance: @exponential_app/ui-solid`, async () => {
  const { report, failures } = await runBrowserSuites<SurfaceContextValue>({
    renderer: { name: pkg.name, platform: `web`, version: pkg.version },
    harness: bunHarness(join(import.meta.dirname, `..`, `harness`, `serve.ts`)),
    logic: { templateItems, itemInstance, runNodeAction },
    log: (line) => console.log(line),
  })
  const out = process.env.EXPONENTIAL_UI_CONFORMANCE_REPORT ?? join(import.meta.dirname, `..`, `..`, `..`, `.conformance`, `exponential-ui-solid.json`)
  mkdirSync(dirname(out), { recursive: true })
  writeFileSync(out, `${JSON.stringify(report, null, 2)}\n`)
  expect(failures).toEqual([])
  const verdict = checkReport(report)
  expect(verdict.problems).toEqual([])
  expect(verdict.conformant).toBe(true)
}, 900_000)
