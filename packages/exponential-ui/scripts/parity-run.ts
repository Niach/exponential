#!/usr/bin/env bun
// FEED-128: `bun run --filter @exponential_app/ui conformance:parity [-- options]`
// — the web parity run (conformance/parity.ts) of a CANDIDATE web renderer's
// harness against the REFERENCE (the React harness), report to `--out`.
//
//   --reference <serve.ts>   default packages/exponential-ui-react/harness/serve.ts
//   --candidate <serve.ts>   default the reference (React vs React = the self-check)
//   --name <candidate name>  default derived from the path (react, solid, …)
//   --out <report.json>      default <repo>/.conformance/exponential-ui-parity-react-<name>.json
//   --only <substring>       only the cases whose id contains it
//   --groups <a,b>           matrix,specimen,state,overlay,primitive (default all)
//   --concurrency <n>        cases at once (default 4)
// Exits non-zero unless every case passes (= parity:check on the report).

import { existsSync, mkdirSync, writeFileSync } from "node:fs"
import { basename, dirname, join, resolve } from "node:path"
import { bunHarness } from "../conformance/browser-page"
import { checkParityReport, parityCases, runParity } from "../conformance/parity"

const argv = process.argv.slice(2)
const arg = (name: string) => {
  const i = argv.indexOf(`--${name}`)
  return i >= 0 ? argv[i + 1] : undefined
}
const repoRoot = join(import.meta.dir, `..`, `..`, `..`)
// `bun run --filter` runs in the package directory: a relative path that is
// not there is taken from the repo root.
const path = (p: string) => (existsSync(resolve(p)) ? resolve(p) : resolve(repoRoot, p))
const reference = path(arg(`reference`) ?? join(repoRoot, `packages/exponential-ui-react/harness/serve.ts`))
const candidate = arg(`candidate`) ? path(arg(`candidate`)!) : reference
const nameOf = (p: string) => basename(dirname(dirname(p))).replace(/^exponential-ui-/, ``)
const name = arg(`name`) ?? nameOf(candidate)
const out = resolve(repoRoot, arg(`out`) ?? join(repoRoot, `.conformance`, `exponential-ui-parity-${nameOf(reference)}-${name}.json`))
const only = arg(`only`)
const groups = arg(`groups`)?.split(`,`)
const cases = (await parityCases()).filter((c) => (!only || c.id.includes(only)) && (!groups || groups.includes(c.group)))

console.error(`parity: ${nameOf(reference)} vs ${name}, ${cases.length} cases`)
const report = await runParity({
  reference: bunHarness(reference),
  candidate: bunHarness(candidate),
  names: { reference: nameOf(reference), candidate: name },
  cases,
  concurrency: arg(`concurrency`) ? Number(arg(`concurrency`)) : undefined,
  log: (line) => console.error(line),
})
mkdirSync(dirname(out), { recursive: true })
writeFileSync(out, `${JSON.stringify(report, null, 2)}\n`)
const problems = checkParityReport(report)
console.log(`${problems.length ? `NO PARITY` : `PARITY`}  ${report.reference} vs ${report.candidate}: ${report.summary.passed}/${report.summary.cases} cases in ${(report.elapsedMs / 1000).toFixed(1)} s → ${out}`)
for (const p of problems.slice(0, 50)) console.log(`  - ${p}`)
process.exit(problems.length ? 1 : 0)
