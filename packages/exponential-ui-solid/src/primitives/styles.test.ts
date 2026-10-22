// FEED-131: ONE stylesheet for both primitive sets. Both packages compile
// packages/exponential-ui-react/src/primitives/tailwind.css (scripts/
// build-css.ts in each); the two dist files must be byte-identical, so a
// class means the same rule on React and on Solid. Spawns both builds (~1 s).

import { spawnSync } from "node:child_process"
import { readFileSync } from "node:fs"
import { join } from "node:path"

const packages = join(import.meta.dirname, `..`, `..`, `..`)
const build = (pkg: string) => {
  const r = spawnSync(`bun`, [`run`, `./scripts/build-css.ts`], { cwd: join(packages, pkg), encoding: `utf8` })
  if (r.status !== 0) throw new Error(`${pkg} build-css failed: ${r.stderr || r.stdout}`)
}

it(`ui-solid ships the ui-react stylesheet byte for byte`, () => {
  build(`exponential-ui-react`)
  build(`exponential-ui-solid`)
  const react = readFileSync(join(packages, `exponential-ui-react`, `dist`, `exponential-ui-react.css`), `utf8`)
  const solid = readFileSync(join(packages, `exponential-ui-solid`, `dist`, `exponential-ui-solid.css`), `utf8`)
  expect(react.length).toBeGreaterThan(1000)
  expect(solid === react).toBe(true)
}, 30_000)
