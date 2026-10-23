// FEED-128: `bun run --filter @exponential-at/ui parity:check <report.json>…`
// — exits non-zero unless every case of every web parity report passed
// (conformance/parity.ts; conformance/README.md "Web parity").
import { existsSync, readFileSync } from "node:fs"
import { join, resolve } from "node:path"
import { checkParityReport, type ParityReport } from "../conformance/parity-compare"

const files = process.argv.slice(2)
if (!files.length) {
  console.error(`usage: parity-check <report.json>…`)
  process.exit(2)
}
let ok = true
for (const file of files) {
  // `bun run --filter` runs in the package directory: a relative path that
  // is not there is taken from the repo root.
  const local = resolve(process.env.INIT_CWD ?? process.cwd(), file)
  const path = existsSync(local) ? local : resolve(join(import.meta.dir, `..`, `..`, `..`), file)
  const report = JSON.parse(readFileSync(path, `utf8`)) as ParityReport
  const problems = checkParityReport(report)
  console.log(`${problems.length ? `NO PARITY` : `PARITY`}  ${report.reference} vs ${report.candidate}: ${(report.cases ?? []).filter((c) => c.ok).length}/${report.cases?.length ?? 0} cases`)
  for (const p of problems) console.log(`  - ${p}`)
  ok &&= problems.length === 0
}
process.exit(ok ? 0 : 1)
