#!/usr/bin/env bun
// VAPP-91: builds an npm package of the SDK into its `dist/`: ONE ESM bundle
// per entry (dependencies external, JSON inlined) + the declarations.
// VAPP-106: the entries share chunks (`splitting`), so a sub-entry never
// duplicates what the root already carries; `bundle()` is the one build the
// release and the size measurement (`size.ts`, minified) both run. The
// workspace package.json files carry NO `sideEffects` flag on purpose: Bun
// 1.3 reads it while bundling the package itself and then drops re-exported
// modules from an entry (the bundle exports names it never declares). The
// published manifest (`prepare-npm.ts`) declares `sideEffects: false` for
// the consumers' bundlers, which is where it belongs.
//
//   bun packages/exponential-ui/release/build-npm.ts packages/exponential-ui
//   bun packages/exponential-ui/release/build-npm.ts packages/exponential-ui-react

import { execSync } from "node:child_process"
import { existsSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs"
import { dirname, join, normalize, resolve } from "node:path"

export const ENTRIES: Record<string, Record<string, string>> = {
  // VAPP-106: the root is what a renderer needs; the prompt (+ the described
  // catalog), the schema emitters, the conformance suite (+ every fixture)
  // and the host API are entries of their own.
  "@exponential-at/ui": { index: `src/index.ts`, prompt: `src/entries/prompt.ts`, schema: `src/entries/schema.ts`, conformance: `src/conformance.ts`, host: `src/host/index.ts` },
  "@exponential-at/ui-react": { index: `src/index.ts`, primitives: `src/primitives/index.ts` },
}

export interface BundleOptions {
  outdir: string
  minify?: boolean
  sourcemap?: boolean
}

export interface BundleOutput {
  /** Output path → the inputs it bundles, their bytes in it, and the chunks it imports. */
  outputs: Record<string, { bytes: number; inputs: Record<string, number>; imports: string[]; entry?: string }>
}

/** The package's entries bundled the way the release ships them: ESM for the
 *  browser, every dependency external, JSON inlined, common code in shared
 *  chunks. Returns the metafile the size measurement reads. */
export async function bundle(dir: string, options: BundleOptions): Promise<BundleOutput> {
  const pkg = JSON.parse(readFileSync(join(dir, `package.json`), `utf8`)) as { name: string; dependencies?: Record<string, string>; peerDependencies?: Record<string, string> }
  const entries = ENTRIES[pkg.name]
  if (!entries) throw new Error(`${pkg.name} is not an npm package of the SDK`)
  const external = [...Object.keys(pkg.dependencies ?? {}), ...Object.keys(pkg.peerDependencies ?? {})].flatMap((d) => [d, `${d}/*`])
  const result = await Bun.build({
    entrypoints: Object.values(entries).map((e) => join(dir, e)),
    outdir: options.outdir,
    root: join(dir, `src`),
    naming: { entry: `[dir]/[name].js`, chunk: `chunks/[name]-[hash].js`, asset: `assets/[name]-[hash].[ext]` },
    format: `esm`,
    target: `browser`,
    splitting: true,
    external,
    minify: options.minify ?? false,
    sourcemap: options.sourcemap ? `linked` : `none`,
    define: { "process.env.NODE_ENV": `"production"` },
    metafile: true,
  } as Parameters<typeof Bun.build>[0])
  if (!result.success) throw new Error(result.logs.map((l) => String(l)).join(`\n`))
  const meta = (result as unknown as { metafile?: { outputs: Record<string, { bytes: number; inputs: Record<string, { bytesInOutput: number }>; imports: { path: string; kind: string }[]; entryPoint?: string }> } }).metafile
  if (!meta) throw new Error(`Bun.build returned no metafile`)
  const outputs: BundleOutput[`outputs`] = {}
  for (const [path, out] of Object.entries(meta.outputs)) {
    if (!path.endsWith(`.js`)) continue
    outputs[path] = {
      bytes: out.bytes,
      inputs: Object.fromEntries(Object.entries(out.inputs).map(([k, v]) => [k, v.bytesInOutput])),
      imports: out.imports.filter((i) => i.kind !== `dynamic-import` || true).map((i) => i.path),
      entry: out.entryPoint,
    }
  }
  return { outputs }
}

export async function buildNpm(dir: string): Promise<void> {
  const pkg = JSON.parse(readFileSync(join(dir, `package.json`), `utf8`)) as { name: string }
  rmSync(join(dir, `dist`, `types`), { recursive: true, force: true })
  for (const f of existsSync(join(dir, `dist`)) ? readdirSync(join(dir, `dist`)) : []) if (f.endsWith(`.js`) || f.endsWith(`.js.map`) || f === `chunks`) rmSync(join(dir, `dist`, f), { recursive: true, force: true })
  const { outputs } = await bundle(dir, { outdir: join(dir, `dist`), sourcemap: true })
  execSync(`bunx tsc -p tsconfig.build.json`, { cwd: dir, stdio: `inherit` })
  fixDeclarationSpecifiers(join(dir, `dist`, `types`))
  console.log(`built ${pkg.name}: ${Object.keys(outputs).map((o) => normalize(o)).join(`, `)}`)
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
