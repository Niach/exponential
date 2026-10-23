#!/usr/bin/env bun
// VAPP-93: every guide's example compiles FROM A FRESH PROJECT. Each check
// copies its guide into a temp dir (nothing of the monorepo's node_modules,
// targets or build caches comes along) and builds it the way a reader would:
//
//   react    the npm packages staged + packed (release/prepare-npm.ts), `npm install`
//            of the tarballs, `tsc --noEmit`, `vite build`, then the extension
//            (defineExtension + its macro reduced) and the vapp (validatePackage)
//            checked at runtime against the packed package
//   solid    the same as react with `@exponential-at/ui-solid` (Vite + vite-plugin-solid),
//            plus the typed DSL from the Solid project (VAPP-108: src/defined.ts run by
//            node must print a valid surface; src/Defined.tsx feeds it to the renderer)
//   element  the same as react with @exponential-at/ui-element (plain Vite, no framework),
//            plus the typed DSL from the plain project (src/defined.ts; src/mount-defined.ts
//            feeds it to <exponential-surface>)
//   agent    the same tarballs, `tsc --noEmit`, then the server started and asked
//            for a valid and an invalid surface over MCP
//   define   the same tarballs, `tsc --noEmit`, then `node surface.ts` must print
//            a valid surface (VAPP-108: the typed authoring DSL)
//   theme    validateTheme + loadTheme over the built-ins: no issues
//   swift    `swift build` against the Swift package by path (macOS; needs
//            packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework)
//   compose  `publishToMavenLocal` of packages/exponential-ui-compose, then
//            `:app:compileDebugKotlin` (needs an Android SDK: ANDROID_HOME)
//   gpui     `cargo check` with the path dependencies made absolute
//
//   bun apps/ui-site/guides/check.ts [react|solid|element|agent|define|theme|swift|compose|gpui|all] [--keep]

import { execFileSync, execSync, spawn } from "node:child_process"
import { cpSync, existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"

const GUIDES = import.meta.dir
const REPO = resolve(GUIDES, `..`, `..`, `..`)
const VERSION = `0.1.0`
const CHECKS = [`react`, `solid`, `element`, `agent`, `define`, `theme`, `gpui`, `swift`, `compose`] as const
type Check = (typeof CHECKS)[number]

const args = process.argv.slice(2)
const keep = args.includes(`--keep`)
const which = args.find((a) => !a.startsWith(`--`)) ?? `all`
if (which !== `all` && !CHECKS.includes(which as Check)) {
  console.error(`usage: check.ts [${CHECKS.join(`|`)}|all] [--keep]`)
  process.exit(2)
}

const temps: string[] = []
function fresh(guide: string): string {
  const dir = mkdtempSync(join(tmpdir(), `xui-guide-${guide}-`))
  temps.push(dir)
  const skip = /\/(node_modules|dist|target|\.build|build|\.gradle|\.kotlin)(\/|$)|\/local\.properties$/
  cpSync(join(GUIDES, guide), dir, { recursive: true, filter: (p) => !skip.test(p.slice(join(GUIDES, guide).length)) })
  console.log(`\n── ${guide}: fresh project at ${dir}`)
  return dir
}

function run(cmd: string, cwd: string, env: Record<string, string> = {}): void {
  console.log(`$ ${cmd}   (${cwd})`)
  execSync(cmd, { cwd, stdio: `inherit`, env: { ...process.env, ...env } })
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
const running = (pattern: string) => {
  try {
    // No shell: `sh -c "pgrep -f …"` would match its own shell (dash does not exec).
    return execFileSync(`pgrep`, [`-f`, pattern], { stdio: [`ignore`, `pipe`, `ignore`] }).toString().trim().length > 0
  } catch {
    return false
  }
}

/** Waits (polling every minute, up to an hour) until `ready()` and no
 *  process matching `busy` runs: a parallel build of a prerequisite. */
async function waitFor(what: string, ready: () => boolean, busy: string): Promise<void> {
  for (let i = 0; i <= 60; i++) {
    if (ready() && !running(busy)) return
    if (i === 0) console.log(`waiting for ${what} (${running(busy) ? `${busy} is running` : `missing`})…`)
    if (i < 60) await sleep(60_000)
  }
  throw new Error(`${what}: still not ready after 60 min`)
}

// ---------------------------------------------------------------------------
// npm tarballs (shared by react, solid, element + agent)

interface Tarballs {
  ui: string
  uiReact: string
  uiSolid: string
  uiElement: string
}
let packed: Tarballs | undefined
async function tarballs(): Promise<Tarballs> {
  if (packed) return packed
  const { prepareNpm } = await import(`../../../packages/exponential-ui/release/prepare-npm`)
  const dir = mkdtempSync(join(tmpdir(), `xui-guide-packs-`))
  temps.push(dir)
  for (const staged of await prepareNpm(VERSION, join(dir, `stage`))) run(`npm pack --silent --pack-destination ${dir}`, staged)
  const files = readdirSync(dir).filter((f) => f.endsWith(`.tgz`))
  const ui = files.find((f) => f === `exponential-at-ui-${VERSION}.tgz`)
  const uiReact = files.find((f) => f === `exponential-at-ui-react-${VERSION}.tgz`)
  const uiSolid = files.find((f) => f === `exponential-at-ui-solid-${VERSION}.tgz`)
  const uiElement = files.find((f) => f === `exponential-at-ui-element-${VERSION}.tgz`)
  if (!ui || !uiReact || !uiSolid || !uiElement) throw new Error(`tarballs missing in ${dir}: ${files.join(`, `)}`)
  packed = { ui: join(dir, ui), uiReact: join(dir, uiReact), uiSolid: join(dir, uiSolid), uiElement: join(dir, uiElement) }
  return packed
}

/** The guide's `^0.1.0` registry deps → the local tarballs (what `npm
 *  install` would fetch once the packages are published). */
function useTarballs(dir: string, t: Tarballs): void {
  const path = join(dir, `package.json`)
  const pkg = JSON.parse(readFileSync(path, `utf8`)) as { dependencies: Record<string, string>; overrides?: Record<string, string> }
  for (const [name, file] of [[`@exponential-at/ui`, t.ui], [`@exponential-at/ui-react`, t.uiReact], [`@exponential-at/ui-solid`, t.uiSolid], [`@exponential-at/ui-element`, t.uiElement]] as const)
    if (pkg.dependencies[name]) pkg.dependencies[name] = `file:${file}`
  // The element depends on ui-solid: one copy of each, the local tarballs.
  pkg.overrides = { "@exponential-at/ui": `file:${t.ui}`, "@exponential-at/ui-solid": `file:${t.uiSolid}` }
  writeFileSync(path, JSON.stringify(pkg, null, 2))
}

const npmInstall = (dir: string) => run(`npm install --no-audit --no-fund --loglevel=error`, dir)

// ---------------------------------------------------------------------------

/** react + solid + element: the same build and the same runtime checks. */
async function webGuide(guide: `react` | `solid` | `element`): Promise<void> {
  const dir = fresh(guide)
  useTarballs(dir, await tarballs())
  npmInstall(dir)
  run(`npx tsc --noEmit`, dir)
  run(`npx vite build --logLevel error`, dir)
  // The JSON halves at runtime, against the PACKED package.
  writeFileSync(
    join(dir, `runtime-check.mjs`),
    `import { readFileSync } from "node:fs"
import { CORE_CATALOG_ID, defineExtension, reduceSurface, validatePackage } from "@exponential-at/ui"
const json = (f) => JSON.parse(readFileSync(new URL(f, import.meta.url), "utf8"))
const problems = []
const ext = defineExtension(json("./src/trendline.extension.json"))
for (const [name, def] of Object.entries(ext.components)) {
  const { root, issues } = reduceSurface([{ id: "root", component: name, ...def.example }], { catalogId: ext.id, extensions: [ext] })
  problems.push(...issues.map((i) => name + ": " + i.id + ": " + i.message))
  const seen = []
  const walk = (n) => { seen.push(n.component); n.children.forEach(walk) }
  walk(root)
  if (seen.includes("Unknown")) problems.push(name + ": expanded to an Unknown placeholder")
  if (def.kind === "macro" && !seen.includes("TrendLine")) problems.push(name + ": the macro did not expand to its TrendLine")
}
const vapp = json("./src/hello.vapp.json")
problems.push(...validatePackage(vapp).map((i) => "hello.vapp.json" + i.path + ": " + i.message))
for (const [id, t] of Object.entries(vapp.templates)) {
  const { issues } = reduceSurface(t.components, { catalogId: vapp.catalogId })
  problems.push(...issues.map((i) => "template " + id + ": " + i.id + ": " + i.message))
}
if (problems.length) { console.error(problems.join("\\n")); process.exit(1) }
console.log("extension + vapp: no issues")
`
  )
  run(`node runtime-check.mjs`, dir)
  // VAPP-108: the typed DSL from this project, unchanged (tsc covered
  // src/defined.ts + src/Defined.tsx above; node runs the DSL half).
  if (existsSync(join(dir, `src`, `defined.ts`))) {
    const out = execFileSync(`node`, [`src/defined.ts`], { cwd: dir, encoding: `utf8` })
    await checkDefinedOutput(out, `${guide}: src/defined.ts`)
  }
}

/** The JSONL a defineSurface script printed: createSurface + updateComponents
 *  (+ updateDataModel), the components valid under the reducer. */
async function checkDefinedOutput(out: string, what: string): Promise<number> {
  const { reduceSurface } = await import(`../../../packages/exponential-ui/src/index.ts`)
  const lines = out.trim().split(`\n`).map((l) => JSON.parse(l) as { createSurface?: unknown; updateComponents?: { catalogId?: string; components: never[] } })
  if (lines.length !== 3 || !lines[0].createSurface || !lines[1].updateComponents) throw new Error(`${what} did not print createSurface + updateComponents + updateDataModel:\n${out}`)
  const { issues } = reduceSurface(lines[1].updateComponents.components, { catalogId: (lines[0].createSurface as { catalogId: string }).catalogId })
  if (issues.length) throw new Error(`${what}: the defined surface has issues: ${issues.map((i) => `${i.id}: ${i.message}`).join(`; `)}`)
  console.log(`${what}: node printed a valid surface (${lines[1].updateComponents.components.length} components)`)
  return lines[1].updateComponents.components.length
}

const react = () => webGuide(`react`)
const solid = () => webGuide(`solid`)
const element = () => webGuide(`element`)

async function agent(): Promise<void> {
  const dir = fresh(`agent`)
  useTarballs(dir, await tarballs())
  npmInstall(dir)
  run(`npx tsc --noEmit`, dir)
  // Start it and ask for one valid and one invalid surface over MCP.
  const port = 4300 + Math.floor(Math.random() * 500) + 100
  const server = spawn(`node`, [`server.ts`], { cwd: dir, env: { ...process.env, PORT: String(port) }, stdio: [`ignore`, `inherit`, `inherit`] })
  try {
    const call = async (body: unknown) => {
      for (let i = 0; i < 50; i++) {
        try {
          const res = await fetch(`http://127.0.0.1:${port}/`, { method: `POST`, headers: { "content-type": `application/json` }, body: JSON.stringify(body) })
          return (await res.json()) as { result: Record<string, unknown> }
        } catch {
          await sleep(200)
        }
      }
      throw new Error(`the agent server did not answer on :${port}`)
    }
    const { messagesFromMcpResult } = await import(`../../../packages/exponential-ui/src/index.ts`)
    const list = await call({ jsonrpc: `2.0`, id: 1, method: `tools/list` })
    const tool = (list.result.tools as { name: string; description: string }[])[0]
    if (tool?.name !== `show_ui` || !tool.description.includes(`createSurface`)) throw new Error(`tools/list: no show_ui carrying the catalog prompt`)
    const good = await call({
      jsonrpc: `2.0`,
      id: 2,
      method: `tools/call`,
      params: { name: `show_ui`, arguments: { components: [{ id: `root`, component: `Card`, padded: true, children: [`t`] }, { id: `t`, component: `Text`, text: `Hello` }] } },
    })
    const decoded = messagesFromMcpResult(good.result)
    if (good.result.isError || decoded.messages.length !== 2 || decoded.issues.length) throw new Error(`a valid surface did not come back as A2UI over MCP: ${JSON.stringify(good.result)}`)
    const bad = await call({ jsonrpc: `2.0`, id: 3, method: `tools/call`, params: { name: `show_ui`, arguments: { components: [{ id: `root`, component: `Text`, text: 42 }] } } })
    if (!bad.result.isError) throw new Error(`an invalid surface was not refused: ${JSON.stringify(bad.result)}`)
    const schema = (await (await fetch(`http://127.0.0.1:${port}/catalog.schema.json`)).json()) as { catalogId?: string }
    if (!schema.catalogId) throw new Error(`/catalog.schema.json: not the catalog schema`)
    console.log(`agent: tools/list, a valid surface (2 A2UI messages), an invalid one refused, the schema served`)
  } finally {
    server.kill()
  }
}

async function define(): Promise<void> {
  const dir = fresh(`define`)
  useTarballs(dir, await tarballs())
  npmInstall(dir)
  run(`npx tsc --noEmit`, dir)
  // The surface it prints must be the valid JSONL a host consumes.
  const out = execFileSync(`node`, [`surface.ts`], { cwd: dir, encoding: `utf8` })
  await checkDefinedOutput(out, `define: surface.ts`)
}

async function theme(): Promise<void> {
  const { BUILTIN_THEMES, loadTheme, validateTheme } = await import(`../../../packages/exponential-ui/src/index.ts`)
  console.log(`\n── theme`)
  const files = readdirSync(join(GUIDES, `theme`)).filter((f) => f.endsWith(`.theme.json`))
  if (!files.length) throw new Error(`no *.theme.json in guides/theme`)
  for (const f of files) {
    const json = JSON.parse(readFileSync(join(GUIDES, `theme`, f), `utf8`))
    const issues = validateTheme(json, { themes: BUILTIN_THEMES })
    if (issues.length) throw new Error(`${f}:\n${issues.map((i) => `  ${i.path}: ${i.message}`).join(`\n`)}`)
    const resolved = loadTheme(json, { themes: BUILTIN_THEMES })
    console.log(`${f}: valid, resolves over ${json.extends ?? `nothing`} (light primary ${resolved.modes.light.color.primary})`)
  }
}

async function swift(): Promise<void> {
  if (process.platform !== `darwin`) throw new Error(`swift: needs macOS (SwiftUI + the xcframework)`)
  const xcf = join(REPO, `packages/exponential-ui-swift/Binaries/ExponentialUIFFI.xcframework`)
  await waitFor(`the xcframework (build-ios.sh)`, () => existsSync(join(xcf, `Info.plist`)), `exponential-ui-ffi/build-ios.sh`)
  const dir = fresh(`swift`)
  const manifest = join(dir, `Package.swift`)
  writeFileSync(manifest, readFileSync(manifest, `utf8`).replace(`"../../../../packages/exponential-ui-swift"`, JSON.stringify(join(REPO, `packages/exponential-ui-swift`))))
  run(`swift build`, dir)
}

async function compose(): Promise<void> {
  const sdk = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT
  if (!sdk) throw new Error(`compose: set ANDROID_HOME to an Android SDK`)
  const painter = join(REPO, `packages/exponential-ui-compose`)
  // Another build of the facade's .so files (not needed to compile against
  // the AAR, but the AAR packages whatever is there) finishes first.
  await waitFor(`the Android facade build (build-android.sh)`, () => true, `exponential-ui-ffi/build-android.sh`)
  // AGP reads the SDK from ANDROID_HOME: no local.properties written anywhere.
  const env = { ANDROID_HOME: sdk }
  const flags = `--console=plain${process.env.CI ? ` --no-daemon` : ``}`
  run(`./gradlew ${flags} :ui-compose:publishReleasePublicationToMavenLocal :ui-compose-primitives:publishReleasePublicationToMavenLocal`, painter, env)
  const dir = fresh(`compose`)
  run(`./gradlew ${flags} :app:compileDebugKotlin`, dir, env)
}

async function gpui(): Promise<void> {
  const dir = fresh(`gpui`)
  const manifest = join(dir, `Cargo.toml`)
  writeFileSync(manifest, readFileSync(manifest, `utf8`).replaceAll(`"../../../../apps/desktop/`, `"${join(REPO, `apps/desktop`)}/`))
  run(`cargo check --quiet`, dir)
}

const RUNNERS: Record<Check, () => Promise<void>> = { react, solid, element, agent, define, theme, gpui, swift, compose }
const failed: string[] = []
for (const name of which === `all` ? CHECKS : [which as Check]) {
  try {
    await RUNNERS[name]()
    console.log(`✓ ${name}`)
  } catch (e) {
    console.error(`✗ ${name}: ${e instanceof Error ? e.message : String(e)}`)
    failed.push(name)
  }
}
if (!keep) for (const t of temps) rmSync(t, { recursive: true, force: true })
else if (temps.length) console.log(`kept: ${temps.join(`, `)}`)
if (failed.length) {
  console.error(`\nguide checks failed: ${failed.join(`, `)}`)
  process.exit(1)
}
console.log(`\nguide checks passed: ${which}`)
