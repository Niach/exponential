#!/usr/bin/env bun
// Postbuild trace repair for the Bun runtime (runs as part of `bun run build`).
//
// Nitro's dependency trace copies react-dom into .output/server/node_modules
// with only the entries it resolved under NODE export conditions
// (server.node.js). At RUNTIME under Bun, `react-dom/server` resolves via the
// "bun" exports condition to server.bun.js — which the trace never copied.
// The nearest-node_modules copy wins resolution, so the shell renderer dies
// with "Cannot find module 'react-dom/server'" and srvx's error path feeds
// Bun.serve a NodeResponse it rejects: every page then serves Bun's default
// placeholder. (Nitro 3 alpha's vite integration has no traceInclude-style
// escape hatch yet — remove this script when it grows one.)
//
// Fix: copy the Bun server entries (and their cjs targets) into the traced
// package after the build.

import { copyFileSync, existsSync, mkdirSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const webRoot = join(dirname(fileURLToPath(import.meta.url)), `..`)
const tracedPkg = join(webRoot, `.output/server/node_modules/react-dom`)

if (!existsSync(tracedPkg)) {
  console.log(`[fix-server-trace] no traced react-dom (nothing to repair)`)
  process.exit(0)
}

// Resolve the real package through the team (hoisted to the repo root).
const sourcePkg = dirname(
  Bun.resolveSync(`react-dom/package.json`, webRoot)
)

const FILES = [
  `server.bun.js`,
  `cjs/react-dom-server.bun.production.js`,
  `cjs/react-dom-server.bun.development.js`,
  `cjs/react-dom-server-legacy.browser.production.js`,
  `cjs/react-dom-server-legacy.browser.development.js`,
]

let copied = 0
for (const file of FILES) {
  const src = join(sourcePkg, file)
  const dest = join(tracedPkg, file)
  if (!existsSync(src)) {
    console.warn(`[fix-server-trace] missing source ${file} — skipped`)
    continue
  }
  if (existsSync(dest)) continue
  mkdirSync(dirname(dest), { recursive: true })
  copyFileSync(src, dest)
  copied += 1
}
console.log(`[fix-server-trace] copied ${copied} react-dom bun entries`)

// Second repair, release train 2026-09-24: rollup (via nitro) can emit an SSR
// chunk whose merged namespace references `attachRouterServerSsrUtils` while
// the chunk only side-effect-imports "@tanstack/router-core/ssr/server" (see
// src/lib/storage/bun-s3-cleanup.ts for the trigger). Every request of the
// built server then dies with a ReferenceError, while the build itself is
// green. Bind the import in place and say so; refuse to finish if a chunk
// references the symbol and no import line is there to bind it.
import { readdirSync, readFileSync, writeFileSync } from "node:fs"

const chunksDir = join(webRoot, `.output/server/chunks/_`)
const SYMBOL = `attachRouterServerSsrUtils`
const SIDE_EFFECT_IMPORT = `import "@tanstack/router-core/ssr/server";`
let repaired = 0
if (existsSync(chunksDir)) {
  for (const name of readdirSync(chunksDir)) {
    if (!name.endsWith(`.mjs`)) continue
    const file = join(chunksDir, name)
    const text = readFileSync(file, `utf8`)
    if (!text.includes(SYMBOL)) continue
    const bound =
      new RegExp(`import \\{[^}]*\\b${SYMBOL}\\b[^}]*\\} from "@tanstack/router-core/ssr/server"`).test(text) ||
      new RegExp(`(function|const|let|var)\\s+${SYMBOL}\\b`).test(text)
    if (bound) continue
    if (!text.includes(SIDE_EFFECT_IMPORT)) {
      console.error(`[fix-server-trace] ${name} references ${SYMBOL} without binding it and has no import line to repair`)
      process.exit(1)
    }
    writeFileSync(file, text.replace(SIDE_EFFECT_IMPORT, `import { ${SYMBOL} } from "@tanstack/router-core/ssr/server";`))
    repaired += 1
    console.warn(`[fix-server-trace] bound ${SYMBOL} in ${name} (rollup left it unbound; a static edge from the server entry into @/lib/storage is the usual cause)`)
  }
}
console.log(`[fix-server-trace] ssr-util repair: ${repaired} chunk(s)`)


// Third repair, release 2026-10-30 (3): a second copy of a package (nitro
// puts it in .nf3/<name>@<version>, the hoisted name links to the other
// version) breaks under Bun in two ways. The trace copied only the `node`
// export targets while Bun takes the sibling `import`/`default` target, and a
// self-import by package name resolves to the hoisted (other) version.
// @noble/hashes@1.8.0 (under @noble/curves) hit both: its esm/utils.js imports
// `@noble/hashes/crypto`, and every request died with "Cannot find module
// '@noble/hashes/crypto'" once sealed-content's x25519 reached the core
// server chunk (#1433). Copy the non-node siblings of every `node` export
// target, and link each such duplicate to itself so self-imports stay inside it.
import { lstatSync, realpathSync, symlinkSync } from "node:fs"
import { relative } from "node:path"

const serverModules = join(webRoot, `.output/server/node_modules`)
const repoRoot = join(webRoot, `../..`)

const versionedCopies = (): string[] => {
  const store = join(serverModules, `.nf3`)
  if (!existsSync(store)) return []
  return readdirSync(store).flatMap((entry) =>
    entry.startsWith(`@`) ? readdirSync(join(store, entry)).map((child) => join(store, entry, child)) : [join(store, entry)]
  ).filter((dir) => existsSync(join(dir, `package.json`)))
}

// Targets Bun may pick where the trace followed `node`: the string leaves of
// every condition object's siblings of a `node` key.
const nonNodeTargets = (exports: unknown, out = new Set<string>()): Set<string> => {
  if (!exports || typeof exports !== `object`) return out
  const entries = Object.entries(exports as Record<string, unknown>)
  const leaves = (value: unknown) => {
    if (typeof value === `string`) {
      if (value.startsWith(`./`) && !value.includes(`*`)) out.add(value)
    } else if (value && typeof value === `object`) for (const v of Object.values(value)) leaves(v)
  }
  if (entries.some(([key]) => key === `node`)) {
    for (const [key, value] of entries) if (key !== `node`) leaves(value)
  }
  for (const [, value] of entries) nonNodeTargets(value, out)
  return out
}

const installedCopy = (name: string, version: string): string | undefined => {
  // The hoisted copy, then the copies nested one level under another package.
  const roots = [join(repoRoot, `node_modules`), join(webRoot, `node_modules`)]
  const candidates = roots.map((root) => join(root, name))
  for (const root of roots.filter(existsSync)) {
    for (const entry of readdirSync(root)) {
      const owners = entry.startsWith(`@`) ? readdirSync(join(root, entry)).map((child) => join(root, entry, child)) : [join(root, entry)]
      for (const owner of owners) candidates.push(join(owner, `node_modules`, name))
    }
  }
  return candidates.find((candidate) => {
    const manifest = join(candidate, `package.json`)
    return existsSync(manifest) && JSON.parse(readFileSync(manifest, `utf8`)).version === version
  })
}

let targetsCopied = 0
let selfLinks = 0
for (const copy of versionedCopies()) {
  const pkg = JSON.parse(readFileSync(join(copy, `package.json`), `utf8`))
  if (!pkg.name || !pkg.version) continue
  const hoisted = join(serverModules, pkg.name)
  if (existsSync(hoisted) && realpathSync(hoisted) === realpathSync(copy)) continue
  const missing = [...nonNodeTargets(pkg.exports)].filter((target) => !existsSync(join(copy, target)))
  const source = missing.length > 0 ? installedCopy(pkg.name, pkg.version) : undefined
  for (const target of missing) {
    if (!source || !existsSync(join(source, target))) continue
    mkdirSync(dirname(join(copy, target)), { recursive: true })
    copyFileSync(join(source, target), join(copy, target))
    targetsCopied += 1
  }
  const self = join(copy, `node_modules`, pkg.name)
  if (existsSync(self) || (() => { try { return lstatSync(self).isSymbolicLink() } catch { return false } })()) continue
  mkdirSync(dirname(self), { recursive: true })
  // Relative, so the link survives the image's move of .output.
  symlinkSync(relative(dirname(self), copy), self, `dir`)
  selfLinks += 1
}
console.log(`[fix-server-trace] duplicate-version repair: ${targetsCopied} export target(s), ${selfLinks} self link(s)`)
