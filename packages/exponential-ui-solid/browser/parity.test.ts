// FEED-128: the web-parity gate (@exponential-at/ui/conformance/parity.ts):
// the React harness = the REFERENCE, the Solid one = the candidate, every
// case compared exactly (DOM, computed styles, frames, pixels). Report to
// EXPONENTIAL_UI_PARITY_REPORT (default
// ../../.conformance/exponential-ui-parity-react-solid.json);
// `checkParityReport` must come back empty.

import { expect, it } from "vitest"
import { mkdirSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { bunHarness } from "../../exponential-ui/conformance/browser-page"
import { checkParityReport, runParity } from "../../exponential-ui/conformance/parity"

const packages = join(import.meta.dirname, `..`, `..`)

it(`Exponential UI web parity: react vs solid`, async () => {
  const report = await runParity({
    reference: bunHarness(join(packages, `exponential-ui-react`, `harness`, `serve.ts`)),
    candidate: bunHarness(join(packages, `exponential-ui-solid`, `harness`, `serve.ts`)),
    names: { reference: `react`, candidate: `solid` },
    log: (line) => console.log(line),
  })
  const out = process.env.EXPONENTIAL_UI_PARITY_REPORT ?? join(packages, `..`, `.conformance`, `exponential-ui-parity-react-solid.json`)
  mkdirSync(dirname(out), { recursive: true })
  writeFileSync(out, `${JSON.stringify(report, null, 2)}\n`)
  console.log(`parity ${report.summary.passed}/${report.summary.cases} → ${out}`)
  expect(checkParityReport(report)).toEqual([])
}, 1_800_000)
