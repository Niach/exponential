#!/usr/bin/env bun
// Recomputes every output in fixtures/<name>.json from the TS reference and
// writes it back. Only after a DELIBERATE spec change: review the diff, it is
// the spec. `bun run scripts/bless.ts [forge|runner|checks|pipeline|remote]`.
import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { REPLAYS, type FixtureName } from "../src/replay"

const names = (process.argv.slice(2).length ? process.argv.slice(2) : Object.keys(REPLAYS)) as FixtureName[]
for (const name of names) {
  const path = join(import.meta.dir, `..`, `fixtures`, `${name}.json`)
  const fixture = JSON.parse(readFileSync(path, `utf8`))
  const replayed = (REPLAYS[name] as (f: unknown) => unknown)(fixture)
  writeFileSync(path, `${JSON.stringify(replayed, null, 2)}\n`)
  console.log(`blessed ${name}`)
}
