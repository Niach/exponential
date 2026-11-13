// #EXP-1804: rewrite the outputs of packages/domain-contract/fixtures/ci-run-view.json
// from web's builders (inputs kept). Review the diff: it is the spec change.
import { readFileSync, writeFileSync } from "node:fs"
import { replayAfter, replayCiRun, replayRunner } from "../src/lib/ci-run-view-replay"

const path = new URL(`../../../packages/domain-contract/fixtures/ci-run-view.json`, import.meta.url)
const fixture = JSON.parse(readFileSync(path, `utf8`))
const json = (value: unknown) => JSON.parse(JSON.stringify(value))
for (const c of fixture.ciRun) c.tree = json(replayCiRun(c))
for (const c of fixture.runner) c.tree = json(replayRunner(c))
for (const c of fixture.after) Object.assign(c, json(replayAfter(c)))
writeFileSync(path, `${JSON.stringify(fixture, null, 1)}\n`)
console.log(`blessed ${fixture.ciRun.length + fixture.runner.length + fixture.after.length} cases`)
