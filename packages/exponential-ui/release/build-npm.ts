#!/usr/bin/env bun
// VAPP-91: builds an npm package of the SDK into its `dist/`: ONE ESM bundle
// per entry (dependencies external, JSON inlined) + the declarations.
//
//   bun packages/exponential-ui/release/build-npm.ts packages/exponential-ui
//   bun packages/exponential-ui/release/build-npm.ts packages/exponential-ui-react

import { execSync } from "node:child_process"
import { existsSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs"
import { dirname, join, resolve } from "node:path"

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
  fixDeclarationSpecifiers(join(dir, `dist`, `types`))
  console.log(`built ${pkg.name}: ${result.outputs.map((o) => o.path.replace(`${dir}/`, ``)).join(`, `)}`)
}

/** Every `.d.ts` under `root`. */
function declarations(root: string): string[] {
  if (!existsSync(root)) return []
  return readdirSync(root).flatMap((name) => {
    const path = join(root, name)
    return statSync(path).isDirectory() ? declarations(path) : name.endsWith(`.d.ts`) ? [path] : []
  })
}

/** VAPP-93: tsc keeps the sources' extensionless relative specifiers in the
 *  declarations, which a consumer on `moduleResolution: node16/nodenext`
 *  cannot follow (every re-export vanishes). Rewrite each to the file it
 *  names: `./x` → `./x.js` or `./x/index.js` (JSON and already-suffixed
 *  specifiers stay). */
export function fixDeclarationSpecifiers(root: string): void {
  const pattern = /(\bfrom\s+|\bimport\s*\(\s*|\bimport\s+)(["'])(\.{1,2}\/[^"']+)\2/g
  for (const file of declarations(root)) {
    const text = readFileSync(file, `utf8`)
    const next = text.replace(pattern, (match, lead: string, quote: string, spec: string) => {
      if (/\.(js|json|mjs|cjs)$/.test(spec)) return match
      const base = resolve(dirname(file), spec)
      if (existsSync(`${base}.d.ts`)) return `${lead}${quote}${spec}.js${quote}`
      if (existsSync(join(base, `index.d.ts`))) return `${lead}${quote}${spec}/index.js${quote}`
      return match
    })
    if (next !== text) writeFileSync(file, next)
  }
}

if (import.meta.main) for (const d of process.argv.slice(2)) await buildNpm(resolve(d))
