#!/usr/bin/env bun
// VAPP-91: builds an npm package of the SDK into its `dist/`: ONE ESM bundle
// per entry (dependencies external, JSON inlined) + the declarations.
//
//   bun packages/exponential-ui/release/build-npm.ts packages/exponential-ui
//   bun packages/exponential-ui/release/build-npm.ts packages/exponential-ui-react
//   bun packages/exponential-ui/release/build-npm.ts packages/exponential-ui-solid
//
// FEED-128: Solid's JSX needs its compiler (Bun's bundler has none), so
// `@exponential-at/ui-solid` builds through Vite + `vite-plugin-solid` in
// library mode into the SAME layout (dist/<entry>.js + dist/types).

import { execSync } from "node:child_process"
import { existsSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs"
import { dirname, join, resolve } from "node:path"

export const ENTRIES: Record<string, Record<string, string>> = {
  "@exponential-at/ui": { index: `src/index.ts`, web: `src/web/index.ts` },
  "@exponential-at/ui-react": { index: `src/index.ts`, primitives: `src/primitives/index.ts` },
  "@exponential-at/ui-solid": { index: `src/index.ts` },
}

/** The packages whose JSX is Solid's (built with Vite, not Bun). */
const SOLID = new Set([`@exponential-at/ui-solid`])

export async function buildNpm(dir: string): Promise<void> {
  const pkg = JSON.parse(readFileSync(join(dir, `package.json`), `utf8`)) as { name: string; dependencies?: Record<string, string>; peerDependencies?: Record<string, string> }
  const entries = ENTRIES[pkg.name]
  if (!entries) throw new Error(`${pkg.name} is not an npm package of the SDK`)
  rmSync(join(dir, `dist`, `index.js`), { force: true })
  rmSync(join(dir, `dist`, `types`), { recursive: true, force: true })
  // The package's own name too: a sub-entry (`@exponential-at/ui/web`)
  // imports its root by name, so the root's module state stays ONE copy.
  const external = [pkg.name, ...Object.keys(pkg.dependencies ?? {}), ...Object.keys(pkg.peerDependencies ?? {})].flatMap((d) => [d, `${d}/*`])
  if (SOLID.has(pkg.name)) {
    const outputs = await buildSolid(dir, entries, external)
    execSync(`bunx tsc -p tsconfig.build.json`, { cwd: dir, stdio: `inherit` })
    fixDeclarationSpecifiers(join(dir, `dist`, `types`))
    console.log(`built ${pkg.name}: ${outputs.join(`, `)}`)
    return
  }
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

/** Vite library mode with `vite-plugin-solid` (DOM output): one ESM file
 *  per entry at dist/<name>.js, every dependency external (`d` and `d/…`). */
async function buildSolid(dir: string, entries: Record<string, string>, external: string[]): Promise<string[]> {
  const { build } = await import(`vite`)
  const { default: solid } = await import(`vite-plugin-solid`)
  const roots = external.filter((d) => !d.endsWith(`/*`))
  // Vite's `build()` sets `process.env.NODE_ENV` to "production" when it is
  // unset, and a child `npm install` spawned later in the same process
  // (guides/check.ts) would then skip devDependencies: restore it.
  const nodeEnv = process.env.NODE_ENV
  const result = await build({
    configFile: false,
    root: dir,
    logLevel: `warn`,
    plugins: [solid()],
    define: { "process.env.NODE_ENV": `"production"` },
    build: {
      outDir: join(dir, `dist`),
      emptyOutDir: false,
      minify: false,
      target: `es2022`,
      sourcemap: true,
      lib: { entry: Object.fromEntries(Object.entries(entries).map(([name, e]) => [name, join(dir, e)])), formats: [`es`], fileName: (_format, name) => `${name}.js` },
      rollupOptions: { external: (id) => roots.some((d) => id === d || id.startsWith(`${d}/`)) },
    },
  })
  if (nodeEnv === undefined) delete process.env.NODE_ENV
  else process.env.NODE_ENV = nodeEnv
  const outputs = (Array.isArray(result) ? result : [result]).flatMap((r) => (`output` in r ? r.output : []))
  return outputs.filter((o) => o.type === `chunk`).map((o) => `dist/${o.fileName}`)
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
