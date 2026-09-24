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

