#!/usr/bin/env bun
// VAPP-106: what the SDK ships, measured. One script, four sections, each
// measuring what its CI job has just built:
//
//   npm          every entry of `build-npm.ts` (bundled MINIFIED into a temp
//                dir, the release ships it readable + source-mapped): the
//                entry file plus every chunk it imports, minified bytes +
//                gzip; each web renderer's `styles.css` as built. Also the
//                root-entry split (SPLIT_RULES): a renderer entry that
//                bundles the described catalog, the prompt, the schema
//                emitters or a fixture fails, `--check` or not.
//   xcframework  packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework:
//                each slice's static library (`build-ios.sh`), the thin
//                archs inside it (lipo).
//   android      apps/desktop/crates/exponential-ui-ffi/out/jniLibs/<abi>/libexponential_ui_ffi.so
//                (`build-android.sh`; an ABI not built is skipped) and the
//                AAR when Gradle has built one.
//   crate        `cargo bloat --crates` on the core's `bench` example (the
//                top crates by .text) + the host cdylib of exponential-ui-ffi
//                in release when it is built.
//
//   bun packages/exponential-ui/release/size.ts [npm|xcframework|android|crate…] [--check] [--write]
//
// `--check` compares against `size-budget.json` (bytes, and gzip where the
// entry has one): anything over its budget fails (exit 1); an entry without
// a budget is reported only. `--write` records the numbers in `sizes.json`
// (the committed record; one section at a time, the others stay). Shrinking
// something = lower its budget in the same commit so the gate ratchets.
// No section = all four (the native ones skip what is not built).

import { execSync } from "node:child_process"
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join, normalize, resolve } from "node:path"
import { gzipSync } from "node:zlib"
import { ENTRIES, STYLESHEET, buildStylesheet, bundle } from "./build-npm"

const repo = resolve(import.meta.dir, `..`, `..`, `..`)
const here = import.meta.dir
const SIZES = join(here, `sizes.json`)
const BUDGET = join(here, `size-budget.json`)

export type Section = `npm` | `xcframework` | `android` | `crate`
export const SECTIONS: readonly Section[] = [`npm`, `xcframework`, `android`, `crate`]

export interface Measure {
  bytes: number
  gzip?: number
  /** What the number is made of (chunks of an entry, thin archs of a slice, the top crates). */
  parts?: Record<string, number>
}

export interface Budget {
  bytes: number
  gzip?: number
}

export interface SizesFile {
  $comment: string
  measured: Partial<Record<Section, { at: string; host: string; entries: Record<string, Measure> }>>
}

const kb = (n: number) => `${(n / 1024).toFixed(1).padStart(7)} KB`
const gz = (text: string | Uint8Array) => gzipSync(text, { level: 9 }).length

// ---------------------------------------------------------------------------
// npm
// ---------------------------------------------------------------------------

/** The packages `build-npm.ts` ships, by directory. */
const NPM_PACKAGES: Record<string, string> = {
  "@exponential-at/ui": `packages/exponential-ui`,
  "@exponential-at/ui-react": `packages/exponential-ui-react`,
  "@exponential-at/ui-solid": `packages/exponential-ui-solid`,
}

/** The entries a RENDERER loads: the root (the runtime catalog, the reducer,
 *  the themes), the host API, the shared web layer and its primitives'
 *  recipes, and both web renderers' entries. */
export const RENDERER_ENTRIES: readonly string[] = [
  `@exponential-at/ui/index`,
  `@exponential-at/ui/host`,
  `@exponential-at/ui/web`,
  `@exponential-at/ui/web/primitives`,
  `@exponential-at/ui-react/index`,
  `@exponential-at/ui-react/primitives`,
  `@exponential-at/ui-solid/index`,
  `@exponential-at/ui-solid/primitives`,
]

/** What a renderer entry must never carry (VAPP-106, the root-entry split):
 *  the catalog's descriptions and examples (`./prompt` and `./schema` attach
 *  them), the prompt, the schema emitters with the vendored A2UI basic
 *  catalog, the conformance suite and every fixture under `fixtures/`
 *  (`./conformance` bundles them). Matched against the bundle's inputs,
 *  repository-relative. */
export const SPLIT_RULES: readonly { name: string; input: RegExp }[] = [
  { name: `the described catalog`, input: /(^|\/)generated\/core\.descriptions\.json$/ },
  { name: `describeCatalog`, input: /(^|\/)src\/catalog\.described\.ts$/ },
  { name: `the prompt`, input: /(^|\/)src\/prompt\.ts$/ },
  { name: `the schema emitters`, input: /(^|\/)src\/schema\.ts$/ },
  { name: `the vendored A2UI catalog`, input: /(^|\/)vendor\/a2ui\// },
  { name: `the conformance suite`, input: /(^|\/)src\/conformance\.ts$/ },
  { name: `the conformance manifest`, input: /(^|\/)conformance\/manifest\.json$/ },
  { name: `a fixture`, input: /(^|\/)fixtures\/[^/]+\.json$/ },
]

/** The split rules a renderer entry breaks, as messages (none for the other entries). */
export function splitViolations(entry: string, inputs: Iterable<string>): string[] {
  if (!RENDERER_ENTRIES.includes(entry)) return []
  const problems: string[] = []
  for (const input of inputs) for (const rule of SPLIT_RULES) if (rule.input.test(input)) problems.push(`${entry} carries ${rule.name}: ${input}`)
  return problems
}

export interface NpmMeasure {
  entries: Record<string, Measure>
  /** The split rules broken (empty when the renderer entries are clean). */
  violations: string[]
}

export async function measureNpm(): Promise<NpmMeasure> {
  const out: Record<string, Measure> = {}
  const violations: string[] = []
  const tmp = mkdtempSync(join(tmpdir(), `exponential-ui-size-`))
  try {
    for (const [name, rel] of Object.entries(NPM_PACKAGES)) {
      const dir = join(repo, rel)
      const outdir = join(tmp, name.replace(`/`, `__`))
      const { outputs } = await bundle(dir, { outdir, minify: true })
      const byPath = new Map(Object.entries(outputs).map(([k, v]) => [normalize(k), v]))
      for (const [entry, source] of Object.entries(ENTRIES[name])) {
        const file = [...byPath.entries()].find(([, v]) => v.entry && resolve(v.entry) === resolve(dir, source))?.[0]
        if (!file) throw new Error(`${name}: no output for ${source}`)
        // The closure: the entry and every chunk it (transitively) imports.
        const closure: string[] = []
        const visit = (p: string) => {
          if (closure.includes(p)) return
          closure.push(p)
          // The metafile names imports relative to the outdir, like the outputs.
          for (const i of byPath.get(p)?.imports ?? []) visit(normalize(i))
        }
        visit(file)
        const texts = closure.map((p) => readFileSync(join(outdir, p)))
        const bytes = texts.reduce((s, t) => s + t.length, 0)
        const parts = Object.fromEntries(closure.map((p, i) => [p.replace(/^\.\//, ``), texts[i].length]))
        const key = `${name}/${entry}`
        out[key] = { bytes, gzip: gz(Buffer.concat(texts)), parts }
        violations.push(...splitViolations(key, closure.flatMap((p) => Object.keys(byPath.get(p)?.inputs ?? {}))))
      }
      const stylesheet = STYLESHEET[name]
      if (stylesheet) {
        buildStylesheet(dir, name)
        const css = readFileSync(join(dir, stylesheet))
        out[`${name}/styles.css`] = { bytes: css.length, gzip: gz(css) }
      }
    }
  } finally {
    rmSync(tmp, { recursive: true, force: true })
  }
  return { entries: out, violations }
}

// ---------------------------------------------------------------------------
// xcframework
// ---------------------------------------------------------------------------

const XCFRAMEWORK = join(repo, `packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework`)

export function measureXcframework(): Record<string, Measure> {
  const out: Record<string, Measure> = {}
  if (!existsSync(XCFRAMEWORK)) return out
  for (const slice of readdirSync(XCFRAMEWORK).sort()) {
    const lib = join(XCFRAMEWORK, slice, `libexponential_ui_ffi.a`)
    if (!existsSync(lib)) continue
    const parts: Record<string, number> = {}
    try {
      // `lipo -detailed_info`: one `architecture <a>` + `size <n>` pair per thin slice.
      const info = execSync(`lipo -detailed_info "${lib}"`, { encoding: `utf8`, stdio: `pipe` })
      let arch = ``
      for (const line of info.split(`\n`)) {
        const a = /^architecture (\S+)/.exec(line.trim())
        const s = /^size (\d+)/.exec(line.trim())
        if (a) arch = a[1]
        else if (s && arch) parts[arch] = Number(s[1])
      }
    } catch {
      // No lipo (not macOS): the slice's bytes stand alone.
    }
    out[`xcframework/${slice}`] = { bytes: statSync(lib).size, ...(Object.keys(parts).length ? { parts } : {}) }
  }
  return out
}

// ---------------------------------------------------------------------------
// android
// ---------------------------------------------------------------------------

const JNI_LIBS = join(repo, `apps/desktop/crates/exponential-ui-ffi/out/jniLibs`)
const AAR_DIR = join(repo, `packages/exponential-ui-compose/ui-compose/build/outputs/aar`)

export function measureAndroid(): Record<string, Measure> {
  const out: Record<string, Measure> = {}
  if (existsSync(JNI_LIBS))
    for (const abi of readdirSync(JNI_LIBS).sort()) {
      const so = join(JNI_LIBS, abi, `libexponential_ui_ffi.so`)
      if (existsSync(so)) out[`android/${abi}`] = { bytes: statSync(so).size }
    }
  if (existsSync(AAR_DIR))
    for (const f of readdirSync(AAR_DIR).sort())
      if (f.endsWith(`.aar`)) out[`android/aar/${f.replace(/\.aar$/, ``)}`] = { bytes: statSync(join(AAR_DIR, f)).size }
  return out
}

// ---------------------------------------------------------------------------
// crate
// ---------------------------------------------------------------------------

const DESKTOP = join(repo, `apps/desktop`)

/** The core's `.text` by crate per `cargo bloat`, on the `bench` example
 *  binary (cargo bloat reads bin/cdylib targets only; the facade has three
 *  crate types), and the host build of the facade (what the Compose JVM
 *  tests load and the bindgen reads). Keyed by the host OS: a Linux `.so`
 *  and a macOS `.dylib` differ. */
export function measureCrate(): Record<string, Measure> {
  const out: Record<string, Measure> = {}
  const host = process.platform
  const targetDir = process.env.CARGO_TARGET_DIR ?? join(DESKTOP, `target`)
  let bloat = ``
  try {
    bloat = execSync(`cargo bloat --release -p exponential-ui --example bench --crates -n 15`, { cwd: DESKTOP, encoding: `utf8`, stdio: [`ignore`, `pipe`, `pipe`] })
  } catch (e) {
    console.warn(`cargo bloat is not available (cargo install cargo-bloat): ${(e as Error).message.split(`\n`)[0]}`)
  }
  const unit = { B: 1, KiB: 1024, MiB: 1024 * 1024 } as Record<string, number>
  const parts: Record<string, number> = {}
  let text = 0
  let file = 0
  for (const line of bloat.split(`\n`)) {
    // " 43.2%  72.6%   1.7MiB exponential_ui" / "59.5% 100.0%  2.4MiB .text section size, the file size is 4.0MiB"
    const m = /^\s*([\d.]+)%\s+([\d.]+)%\s+([\d.]+)(B|KiB|MiB)\s+(.+?)\s*$/.exec(line)
    if (!m) continue
    const n = Math.round(Number(m[3]) * unit[m[4]]!)
    if (m[5].startsWith(`.text section size`)) {
      text = n
      const f = /the file size is ([\d.]+)(B|KiB|MiB)/.exec(m[5])
      if (f) file = Math.round(Number(f[1]) * unit[f[2]]!)
    } else if (!m[5].startsWith(`And `)) parts[m[5]] = n
  }
  const bench = join(targetDir, `release`, `examples`, `bench`)
  if (existsSync(bench)) file = statSync(bench).size
  if (file) out[`crate/${host}/bench-example`] = { bytes: file, ...(text ? { parts: { ".text": text, ...parts } } : {}) }
  const lib = join(targetDir, `release`, host === `darwin` ? `libexponential_ui_ffi.dylib` : `libexponential_ui_ffi.so`)
  if (existsSync(lib)) out[`crate/${host}/exponential-ui-ffi`] = { bytes: statSync(lib).size }
  return out
}

// ---------------------------------------------------------------------------
// the record, the budget, the report
// ---------------------------------------------------------------------------

export function readSizes(): SizesFile {
  return existsSync(SIZES) ? (JSON.parse(readFileSync(SIZES, `utf8`)) as SizesFile) : { $comment: ``, measured: {} }
}

export function readBudget(): Record<string, Budget> {
  const file = JSON.parse(readFileSync(BUDGET, `utf8`)) as Record<string, Budget | string>
  return Object.fromEntries(Object.entries(file).filter(([k]) => !k.startsWith(`$`))) as Record<string, Budget>
}

/** Every entry over its budget, as a message. */
export function overBudget(entries: Record<string, Measure>, budget: Record<string, Budget>): string[] {
  const problems: string[] = []
  for (const [key, m] of Object.entries(entries)) {
    const b = budget[key]
    if (!b) continue
    if (m.bytes > b.bytes) problems.push(`${key}: ${m.bytes} bytes, budget ${b.bytes} (+${m.bytes - b.bytes})`)
    if (b.gzip !== undefined && m.gzip !== undefined && m.gzip > b.gzip) problems.push(`${key}: ${m.gzip} bytes gzipped, budget ${b.gzip} (+${m.gzip - b.gzip})`)
  }
  return problems
}

function report(section: Section, entries: Record<string, Measure>, budget: Record<string, Budget>, previous?: Record<string, Measure>): void {
  console.log(`== ${section}`)
  if (!Object.keys(entries).length) {
    console.log(`  (nothing built)`)
    return
  }
  for (const [key, m] of Object.entries(entries)) {
    const b = budget[key]
    const p = previous?.[key]
    const delta = p ? ` (${m.bytes >= p.bytes ? `+` : ``}${((m.bytes - p.bytes) / 1024).toFixed(1)} KB vs the record)` : ``
    console.log(`  ${key.padEnd(44)} ${kb(m.bytes)}${m.gzip !== undefined ? `  gzip ${kb(m.gzip)}` : ``}${b ? `  budget ${kb(b.bytes)}${b.gzip !== undefined ? ` / ${kb(b.gzip)}` : ``}` : `  (no budget)`}${delta}`)
    for (const [part, n] of Object.entries(m.parts ?? {}).slice(0, 12)) console.log(`      ${part.padEnd(40)} ${kb(n)}`)
  }
}

export async function run(sections: Section[], check: boolean, write: boolean): Promise<boolean> {
  const sizes = readSizes()
  const budget = readBudget()
  let ok = true
  for (const section of sections) {
    const npm = section === `npm` ? await measureNpm() : undefined
    const entries = npm ? npm.entries : section === `xcframework` ? measureXcframework() : section === `android` ? measureAndroid() : measureCrate()
    report(section, entries, budget, sizes.measured[section]?.entries)
    for (const v of npm?.violations ?? []) console.log(`  SPLIT  ${v}`)
    if (npm?.violations.length) ok = false
    const problems = check ? overBudget(entries, budget) : []
    for (const p of problems) console.log(`  OVER BUDGET  ${p}`)
    if (problems.length) ok = false
    if (write && Object.keys(entries).length) {
      // A section measured partially (one ABI, no AAR) keeps the record's other entries.
      const kept = sizes.measured[section]?.entries ?? {}
      sizes.measured[section] = { at: new Date().toISOString().slice(0, 10), host: `${process.platform}-${process.arch}`, entries: { ...kept, ...entries } }
    }
  }
  if (write) {
    sizes.$comment = `VAPP-106: the SDK's sizes on record, written by release/size.ts --write (bytes; gzip for what a browser downloads; parts = the chunks of an entry, the thin archs of a slice, the top crates by .text). The budgets live in size-budget.json; CI (exponential-ui.yml) fails a pull request that goes over one.`
    writeFileSync(SIZES, `${JSON.stringify(sizes, null, 2)}\n`)
    console.log(`wrote ${SIZES}`)
  }
  return ok
}

if (import.meta.main) {
  const args = process.argv.slice(2)
  const check = args.includes(`--check`)
  const write = args.includes(`--write`)
  const picked = args.filter((a) => !a.startsWith(`--`)) as Section[]
  for (const s of picked)
    if (!SECTIONS.includes(s)) {
      console.error(`unknown section ${s}; one of ${SECTIONS.join(`|`)}`)
      process.exit(2)
    }
  const ok = await run(picked.length ? picked : [...SECTIONS], check, write)
  process.exit(ok ? 0 : 1)
}
