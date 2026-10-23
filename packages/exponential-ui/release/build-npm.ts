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
//   bun packages/exponential-ui/release/build-npm.ts packages/exponential-ui-solid
//   bun packages/exponential-ui/release/build-npm.ts packages/exponential-ui-element
//
// FEED-128: Solid's JSX needs its compiler (Bun's bundler has none), so
// `@exponential_app/ui-solid` builds through Vite + `vite-plugin-solid` in
// library mode into the SAME layout (dist/<entry>.js + dist/types).
//
// FEED-130: `@exponential_app/ui-element` has no JSX (it mounts ui-solid, a
// dependency, so external): the plain Bun path with code splitting (`.` and
// `./define` share ONE element class), then `scripts/build-standalone.ts`
// writes dist/standalone.js, everything inlined.
// FEED-131: both web renderers ship their primitives' stylesheet
// (`./styles.css`): the build runs the package's `build:css` into dist/.

import { execSync } from "node:child_process"
import { existsSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs"
import { dirname, join, normalize, relative, resolve } from "node:path"

/** The repository root: the metafile names every input relative to it (the
 *  way Bun names them from a build run at the root), whichever bundler ran. */
const REPO = resolve(import.meta.dir, `..`, `..`, `..`)
const repoRelative = (path: string): string => relative(REPO, resolve(path)).split(`\\`).join(`/`)

export const ENTRIES: Record<string, Record<string, string>> = {
  // VAPP-106: the root is what a renderer needs; the prompt (+ the described
  // catalog), the schema emitters, the conformance suite (+ every fixture)
  // and the host API are entries of their own. FEED-128/131: the web layer
  // and its primitives' recipes are entries the web renderers import.
  "@exponential_app/ui": {
    index: `src/index.ts`,
    prompt: `src/entries/prompt.ts`,
    schema: `src/entries/schema.ts`,
    conformance: `src/conformance.ts`,
    host: `src/host/index.ts`,
    web: `src/web/index.ts`,
    "web/primitives": `src/web/primitives/index.ts`,
  },
  "@exponential_app/ui-react": { index: `src/index.ts`, primitives: `src/primitives/index.ts` },
  "@exponential_app/ui-solid": { index: `src/index.ts`, primitives: `src/primitives/index.ts` },
  // FEED-130: `.` and `./define` share one element class through a chunk;
  // `dist/standalone.js` (everything inlined) is built by the package's script.
  "@exponential_app/ui-element": { index: `src/index.ts`, define: `src/define.ts` },
}

/** The packages that also ship a self-contained bundle (`scripts/build-standalone.ts` → dist/standalone.js). */
export const STANDALONE: Record<string, string> = {
  "@exponential_app/ui-element": `dist/standalone.js`,
}

export interface StandaloneBundle {
  /** The built file. */
  file: string
  /** Repository-relative input → its bytes in the file. */
  inputs: Record<string, number>
}

/** The package's standalone bundle through its own script, into `outDir`
 *  (minified; the release adds a sourcemap). */
export async function bundleStandalone(dir: string, options: { outDir: string; sourcemap?: boolean }): Promise<StandaloneBundle> {
  const { bundleStandalone: run } = (await import(join(dir, `scripts`, `build-standalone.ts`))) as { bundleStandalone: (dir: string, o: { outDir: string; sourcemap?: boolean }) => Promise<StandaloneBundle> }
  const out = await run(dir, options)
  return { file: out.file, inputs: Object.fromEntries(Object.entries(out.inputs).map(([k, v]) => [repoRelative(k), v])) }
}

/** The packages that ship a compiled stylesheet (`build:css` → dist/<name>.css). */
export const STYLESHEET: Record<string, string> = {
  "@exponential_app/ui-react": `dist/exponential-ui-react.css`,
  "@exponential_app/ui-solid": `dist/exponential-ui-solid.css`,
}

/** The packages whose JSX is Solid's (built with Vite, not Bun). */
const SOLID = new Set([`@exponential_app/ui-solid`])

export interface BundleOptions {
  outdir: string
  minify?: boolean
  sourcemap?: boolean
}

export interface BundleOutput {
  /** Output path → the inputs it bundles, their bytes in it, and the chunks it imports. */
  outputs: Record<string, { bytes: number; inputs: Record<string, number>; imports: string[]; entry?: string }>
}

interface Manifest {
  name: string
  dependencies?: Record<string, string>
  peerDependencies?: Record<string, string>
}

/** Every dependency and peer (`d` and `d/*`), and the package's own name:
 *  a sub-entry (`@exponential_app/ui/web`) imports its root by name, so the
 *  root's module state stays ONE copy. */
function externals(pkg: Manifest): string[] {
  return [pkg.name, ...Object.keys(pkg.dependencies ?? {}), ...Object.keys(pkg.peerDependencies ?? {})].flatMap((d) => [d, `${d}/*`])
}

/** The package's entries bundled the way the release ships them: ESM for the
 *  browser, every dependency external, JSON inlined, common code in shared
 *  chunks (Bun) or one file per entry (Vite, for Solid). Returns the metafile
 *  the size measurement reads. */
export async function bundle(dir: string, options: BundleOptions): Promise<BundleOutput> {
  const pkg = JSON.parse(readFileSync(join(dir, `package.json`), `utf8`)) as Manifest
  const entries = ENTRIES[pkg.name]
  if (!entries) throw new Error(`${pkg.name} is not an npm package of the SDK`)
  const external = externals(pkg)
  if (SOLID.has(pkg.name)) return buildSolid(dir, entries, external, options)
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
      inputs: Object.fromEntries(Object.entries(out.inputs).map(([k, v]) => [repoRelative(k), v.bytesInOutput])),
      imports: out.imports.filter((i) => i.kind !== `dynamic-import` || true).map((i) => i.path),
      entry: out.entryPoint,
    }
  }
  return { outputs }
}

export async function buildNpm(dir: string): Promise<void> {
  const pkg = JSON.parse(readFileSync(join(dir, `package.json`), `utf8`)) as Manifest
  rmSync(join(dir, `dist`, `types`), { recursive: true, force: true })
  for (const f of existsSync(join(dir, `dist`)) ? readdirSync(join(dir, `dist`)) : []) if (f.endsWith(`.js`) || f.endsWith(`.js.map`) || f === `chunks`) rmSync(join(dir, `dist`, f), { recursive: true, force: true })
  const { outputs } = await bundle(dir, { outdir: join(dir, `dist`), sourcemap: true })
  execSync(`bunx tsc -p tsconfig.build.json`, { cwd: dir, stdio: `inherit` })
  fixDeclarationSpecifiers(join(dir, `dist`, `types`))
  const built = Object.keys(outputs).map((o) => normalize(o))
  if (STANDALONE[pkg.name]) built.push((await bundleStandalone(dir, { outDir: join(dir, `dist`), sourcemap: true })).file.replace(`${dir}/`, ``))
  buildStylesheet(dir, pkg.name)
  console.log(`built ${pkg.name}: ${built.join(`, `)}`)
}

/** The package's `build:css`, when it ships a stylesheet. */
export function buildStylesheet(dir: string, name: string): void {
  if (STYLESHEET[name]) execSync(`bun run build:css`, { cwd: dir, stdio: `pipe` })
}

/** Vite library mode with `vite-plugin-solid` (DOM output): one ESM file
 *  per entry at <outdir>/<name>.js, every dependency external (`d` and
 *  `d/…`), the entries' shared code in chunks like the Bun build. */
async function buildSolid(dir: string, entries: Record<string, string>, external: string[], options: BundleOptions): Promise<BundleOutput> {
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
      outDir: options.outdir,
      emptyOutDir: false,
      minify: options.minify ? `esbuild` : false,
      target: `es2022`,
      sourcemap: options.sourcemap ?? false,
      lib: { entry: Object.fromEntries(Object.entries(entries).map(([name, e]) => [name, join(dir, e)])), formats: [`es`], fileName: (_format, name) => `${name}.js` },
      rollupOptions: { external: (id) => roots.some((d) => id === d || id.startsWith(`${d}/`)), output: { chunkFileNames: `chunks/[name]-[hash].js` } },
    },
  })
  if (nodeEnv === undefined) delete process.env.NODE_ENV
  else process.env.NODE_ENV = nodeEnv
  const chunks = (Array.isArray(result) ? result : [result]).flatMap((r) => (`output` in r ? r.output : [])).filter((o) => o.type === `chunk`)
  const outputs: BundleOutput[`outputs`] = {}
  for (const c of chunks) {
    const path = `./${c.fileName}`
    outputs[path] = {
      bytes: Buffer.byteLength(c.code),
      inputs: Object.fromEntries(Object.entries(c.modules).map(([k, v]) => [repoRelative(k), v.renderedLength])),
      imports: c.imports.filter((i) => chunks.some((o) => o.fileName === i)).map((i) => `./${i}`),
      entry: c.isEntry && c.facadeModuleId ? c.facadeModuleId : undefined,
    }
  }
  return { outputs }
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
