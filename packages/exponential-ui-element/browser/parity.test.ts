// FEED-130: the web-parity gate (@exponential_app/eui/conformance/parity.ts):
// the React harness = the REFERENCE, the element one = the candidate, every
// case compared exactly (DOM, computed styles, frames, pixels). Report to
// EXPONENTIAL_UI_PARITY_REPORT (default
// ../../.conformance/exponential-ui-parity-react-element.json);
// `checkParityReport` must come back empty. The renderer groups only
// (matrix, specimen, state, overlay): the `primitive` group (FEED-131)
// compares primitive SETS, and the element is a renderer without one.

import { expect, it } from "vitest"
import { mkdirSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { bunHarness } from "../../exponential-ui/conformance/browser-page"
import { checkParityReport, parityCases, runParity } from "../../exponential-ui/conformance/parity"

const packages = join(import.meta.dirname, `..`, `..`)

it(`Exponential UI web parity: react vs element`, async () => {
  const cases = (await parityCases()).filter((c) => c.group !== `primitive`)
  const report = await runParity({
    cases,
    reference: bunHarness(join(packages, `exponential-ui-react`, `harness`, `serve.ts`)),
    candidate: bunHarness(join(packages, `exponential-ui-element`, `harness`, `serve.ts`)),
    names: { reference: `react`, candidate: `element` },
    log: (line) => console.log(line),
  })
  const out = process.env.EXPONENTIAL_UI_PARITY_REPORT ?? join(packages, `..`, `.conformance`, `exponential-ui-parity-react-element.json`)
  mkdirSync(dirname(out), { recursive: true })
  writeFileSync(out, `${JSON.stringify(report, null, 2)}\n`)
  console.log(`parity ${report.summary.passed}/${report.summary.cases} → ${out}`)
  expect(checkParityReport(report)).toEqual([])
}, 1_800_000)
