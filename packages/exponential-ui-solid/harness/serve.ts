#!/usr/bin/env bun
// FEED-128: builds harness/main.tsx with Vite (`vite-plugin-solid`: Solid's
// JSX needs its compiler, Bun's bundler has none) into a temp dir on the
// first request and serves it (`PORT`, default 4182) with the SAME page as
// the React harness. The browser suites spawn this; `dev:harness` runs it
// for a human.

import { mkdtempSync, readFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { build } from "vite"
import solid from "vite-plugin-solid"

const root = import.meta.dir
const port = Number(process.env.PORT ?? 4182)

/** The harness bundle (one ES module, every import inlined). */
export async function bundleHarness(): Promise<string> {
  const outDir = mkdtempSync(join(tmpdir(), `xui-solid-harness-`))
  await build({
    configFile: false,
    root,
    logLevel: `warn`,
    plugins: [solid()],
    resolve: { conditions: [`browser`] },
    define: { "process.env.NODE_ENV": JSON.stringify(`production`) },
    build: {
      outDir,
      emptyOutDir: true,
      minify: false,
      target: `es2022`,
      modulePreload: false,
      rollupOptions: {
        input: join(root, `main.tsx`),
        output: { format: `es`, entryFileNames: `main.js`, inlineDynamicImports: true },
      },
    },
  })
  return readFileSync(join(outDir, `main.js`), `utf8`)
}

const PAGE = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Exponential UI · harness</title></head><body><div id="app"></div><script type="module" src="/dist/main.js"></script></body></html>`

if (import.meta.main) {
  let cached: Promise<string> | null = null
  Bun.serve({
    port,
    async fetch(req) {
      const url = new URL(req.url)
      if (url.pathname === `/` || url.pathname === `/index.html`) return new Response(PAGE, { headers: { "content-type": `text/html; charset=utf-8` } })
      if (url.pathname === `/dist/main.js`) {
        try {
          cached ??= bundleHarness()
          return new Response(await cached, { headers: { "content-type": `text/javascript; charset=utf-8`, "cache-control": `no-store` } })
        } catch (error) {
          cached = null
          return new Response(String(error), { status: 500 })
        }
      }
      return new Response(`Not found`, { status: 404 })
    },
  })
  console.log(`harness  http://localhost:${port}/?view=kitchen-sink`)
}
