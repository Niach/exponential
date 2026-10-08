// VAPP-91: `bun run --filter @exponential-at/ui conformance:check <report.json>…`
// — exits non-zero unless every report is conformant (conformance/README.md).
import { readFileSync } from "node:fs"
import { checkReport, conformanceManifest } from "../src/conformance"
import type { ConformanceReport } from "../src/conformance"

const files = process.argv.slice(2)
if (!files.length) {
  console.error(`usage: conformance-check <report.json>…`)
  process.exit(2)
}
const manifest = conformanceManifest()
let ok = true
for (const file of files) {
  const report = JSON.parse(readFileSync(file, `utf8`)) as ConformanceReport
  const verdict = checkReport(report, manifest)
  const total = Object.values(report.suites ?? {}).reduce((s, r) => s + r.passed, 0)
  console.log(`${verdict.conformant ? `CONFORMANT` : `NOT CONFORMANT`}  ${report.renderer} (${report.platform} ${report.version}): ${total} cases passed in ${Object.keys(report.suites ?? {}).length} suites`)
  for (const p of verdict.problems) console.log(`  - ${p}`)
  ok &&= verdict.conformant
}
process.exit(ok ? 0 : 1)
