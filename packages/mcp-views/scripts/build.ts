// EXP-1153: build the MCP App views the web app serves as `ui://` resources.
//
//   bun run --filter @exp/mcp-views build
//
// Per view: a Vite IIFE bundle (React + the @exp/ui components it uses), the
// package stylesheet compiled ONCE by the island compiler (Tailwind scanned
// over this package and @exp/ui), and one self-contained HTML document in
// dist/<view>.html — the exact bytes `resources/read` returns. The web
// server reads those files at request time (lib/mcp/views.ts); Docker builds
// them before the web app.

import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { resolve } from "node:path"
import { build } from "vite"
import { compileUiCss } from "@exp/ui/island"
import { VIEW_NAMES, VIEW_TITLES } from "../src/contract"

const root = resolve(import.meta.dir, `..`)
const dist = resolve(root, `dist`)

rmSync(resolve(dist, `js`), { recursive: true, force: true })
mkdirSync(dist, { recursive: true })

const css = await compileUiCss({ base: root })

for (const view of VIEW_NAMES) {
  await build({ configFile: resolve(root, `vite.config.ts`), mode: view })
  const jsPath = resolve(dist, `js`, `${view}.js`)
  if (!existsSync(jsPath)) throw new Error(`vite produced no ${jsPath}`)
  // The bundle is inlined into a classic <script>: the only sequence that
  // could end it early is a literal `</script`, which JS strings survive as
  // `<\/script`.
  const js = readFileSync(jsPath, `utf8`).replace(/<\/script/gi, `<\\/script`)
  const html = [
    `<!doctype html>`,
    `<html lang="en">`,
    `<head>`,
    `<meta charset="utf-8">`,
    `<meta name="viewport" content="width=device-width, initial-scale=1">`,
    `<title>${VIEW_TITLES[view]} · Exponential</title>`,
    `<style>${css}</style>`,
    // The web app's 18.5px root is its own choice; inside a host's
    // conversation the view reads at the host's size.
    `<style>html{font-size:15px}body{margin:0;background:transparent}</style>`,
    `</head>`,
    `<body><div id="root"></div>`,
    `<script>${js}</script>`,
    `</body></html>`,
  ].join(`\n`)
  const out = resolve(dist, `${view}.html`)
  writeFileSync(out, html)
  console.log(`wrote ${out} (${(html.length / 1024).toFixed(0)} KiB)`)
}
