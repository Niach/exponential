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

// Resolve the real package through the workspace (hoisted to the repo root).
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
