#!/usr/bin/env bun
// FEED-130: builds harness/main.ts with Vite in memory on the first
// request and serves it (`PORT`, default 4183) with the SAME page as the
// React and Solid harnesses. The harness itself has no JSX (it drives the
// custom element); `vite-plugin-solid` compiles the Solid painter, which
// `@exponential_app/ui-solid` ships as TSX sources inside the monorepo, and
// harness/vlist.tsx. The browser suites spawn this; `dev:harness` runs it
// for a human.

import { join } from "node:path"
import { build } from "vite"
import solid from "vite-plugin-solid"

const root = import.meta.dir
const port = Number(process.env.PORT ?? 4183)

/** The harness bundle (one ES module, every import inlined), built in
 *  memory (`write: false`): nothing lands on disk, so no run leaves a
 *  bundle behind in the system temp folder (VAPP-150). */
export async function bundleHarness(): Promise<string> {
  const result = await build({
    configFile: false,
    root,
    logLevel: `warn`,
    plugins: [solid()],
    resolve: { conditions: [`browser`] },
    define: { "process.env.NODE_ENV": JSON.stringify(`production`) },
    build: {
      write: false,
      minify: false,
      target: `es2022`,
      modulePreload: false,
      rollupOptions: {
        input: join(root, `main.ts`),
        output: { format: `es`, entryFileNames: `main.js`, inlineDynamicImports: true },
      },
    },
  })
  const outputs = (Array.isArray(result) ? result : [result]).flatMap((r) => (`output` in r ? r.output : []))
  const chunk = outputs.find((o) => o.type === `chunk` && o.fileName === `main.js`)
  if (!chunk || chunk.type !== `chunk`) throw new Error(`the harness build produced no main.js`)
  return chunk.code
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
