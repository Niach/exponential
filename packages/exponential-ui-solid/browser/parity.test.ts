// FEED-128: the web-parity gate (@exponential_app/eui/conformance/parity.ts):
// the React harness = the REFERENCE, the Solid one = the candidate, every
// case compared exactly (DOM, computed styles, frames, pixels). Report to
// EXPONENTIAL_UI_PARITY_REPORT (default
// ../../.conformance/exponential-ui-parity-react-solid.json);
// `checkParityReport` must come back empty. One run covers every group:
// the renderer's (matrix, specimen, state, overlay) AND the primitive sets'
// (`primitive`, FEED-131: harness/primitives/, fixtures/
// primitive-specimens.json). EXPONENTIAL_UI_PARITY_GROUP=<group>[,<group>]
// keeps only those groups (`test:parity:primitives` = `primitive`, seconds).

import { expect, it } from "vitest"
import { mkdirSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { bunHarness } from "../../exponential-ui/conformance/browser-page"
import { checkParityReport, parityCases, runParity } from "../../exponential-ui/conformance/parity"

const packages = join(import.meta.dirname, `..`, `..`)

const groups = process.env.EXPONENTIAL_UI_PARITY_GROUP?.split(`,`).filter(Boolean)

it(`Exponential UI web parity: react vs solid`, async () => {
  const cases = groups?.length ? (await parityCases()).filter((c) => groups.includes(c.group)) : undefined
  const report = await runParity({
    cases,
    reference: bunHarness(join(packages, `exponential-ui-react`, `harness`, `serve.ts`)),
    candidate: bunHarness(join(packages, `exponential-ui-solid`, `harness`, `serve.ts`)),
    names: { reference: `react`, candidate: `solid` },
    log: (line) => console.log(line),
  })
  // A group-filtered run reports beside the full one, never over it.
  const out = process.env.EXPONENTIAL_UI_PARITY_REPORT ?? join(packages, `..`, `.conformance`, `exponential-ui-parity-react-solid${groups?.length ? `.${groups.join(`-`)}` : ``}.json`)
  mkdirSync(dirname(out), { recursive: true })
  writeFileSync(out, `${JSON.stringify(report, null, 2)}\n`)
  console.log(`parity ${report.summary.passed}/${report.summary.cases} → ${out}`)
  expect(checkParityReport(report)).toEqual([])
}, 1_800_000)
