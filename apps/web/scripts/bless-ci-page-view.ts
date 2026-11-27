// #EXP-2172: rewrite the trees of packages/domain-contract/fixtures/ci-page-view.json
// from web's builder (inputs kept). Review the diff: it is the spec change.
import { readFileSync, writeFileSync } from "node:fs"
import { replayCiPage } from "../src/lib/ci-page-view-replay"

const path = new URL(`../../../packages/domain-contract/fixtures/ci-page-view.json`, import.meta.url)
const fixture = JSON.parse(readFileSync(path, `utf8`))
const json = (value: unknown) => JSON.parse(JSON.stringify(value))
for (const c of fixture.cases) c.tree = json(replayCiPage(c))
writeFileSync(path, `${JSON.stringify(fixture, null, 1)}\n`)
console.log(`blessed ${fixture.cases.length} cases`)
