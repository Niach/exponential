#!/usr/bin/env bun
// VAPP-91: builds an npm package of the SDK into its `dist/`: ONE ESM bundle
// per entry (dependencies external, JSON inlined) + the declarations.
//
//   bun packages/exponential-ui/release/build-npm.ts packages/exponential-ui
//   bun packages/exponential-ui/release/build-npm.ts packages/exponential-ui-react

import { execSync } from "node:child_process"
import { readFileSync, rmSync } from "node:fs"
import { join, resolve } from "node:path"

export const ENTRIES: Record<string, Record<string, string>> = {
  "@exponential-at/ui": { index: `src/index.ts` },
  "@exponential-at/ui-react": { index: `src/index.ts`, primitives: `src/primitives/index.ts` },
}

export async function buildNpm(dir: string): Promise<void> {
  const pkg = JSON.parse(readFileSync(join(dir, `package.json`), `utf8`)) as { name: string; dependencies?: Record<string, string>; peerDependencies?: Record<string, string> }
  const entries = ENTRIES[pkg.name]
  if (!entries) throw new Error(`${pkg.name} is not an npm package of the SDK`)
  rmSync(join(dir, `dist`, `index.js`), { force: true })
  rmSync(join(dir, `dist`, `types`), { recursive: true, force: true })
  const external = [...Object.keys(pkg.dependencies ?? {}), ...Object.keys(pkg.peerDependencies ?? {})].flatMap((d) => [d, `${d}/*`])
  const result = await Bun.build({
    entrypoints: Object.values(entries).map((e) => join(dir, e)),
    outdir: join(dir, `dist`),
    root: join(dir, `src`),
    naming: `[dir]/[name].js`,
    format: `esm`,
    target: `browser`,
    external,
    sourcemap: `linked`,
    define: { "process.env.NODE_ENV": `"production"` },
  })
  if (!result.success) throw new Error(result.logs.map((l) => String(l)).join(`\n`))
  execSync(`bunx tsc -p tsconfig.build.json`, { cwd: dir, stdio: `inherit` })
  console.log(`built ${pkg.name}: ${result.outputs.map((o) => o.path.replace(`${dir}/`, ``)).join(`, `)}`)
}

if (import.meta.main) for (const d of process.argv.slice(2)) await buildNpm(resolve(d))
