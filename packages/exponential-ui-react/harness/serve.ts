#!/usr/bin/env bun
// VAPP-87: bundles harness/main.tsx with Bun on every request and serves it
// (`PORT`, default 4181). The browser suites spawn this; `dev:harness` runs
// it for a human.

import { join } from "node:path"

const root = import.meta.dir
const port = Number(process.env.PORT ?? 4181)

export async function bundleHarness(): Promise<string> {
  const result = await Bun.build({
    entrypoints: [join(root, `main.tsx`)],
    target: `browser`,
    format: `esm`,
    minify: false,
    define: { "process.env.NODE_ENV": JSON.stringify(`production`) },
  })
  if (!result.success) throw new Error(result.logs.map((l) => l.message).join(`\n`))
  return await result.outputs[0].text()
}

const PAGE = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Exponential UI · harness</title></head><body><div id="app"></div><script type="module" src="/dist/main.js"></script></body></html>`

if (import.meta.main) {
  let cached: string | null = null
  Bun.serve({
    port,
    async fetch(req) {
      const url = new URL(req.url)
      if (url.pathname === `/` || url.pathname === `/index.html`) return new Response(PAGE, { headers: { "content-type": `text/html; charset=utf-8` } })
      if (url.pathname === `/dist/main.js`) {
        try {
          cached ??= await bundleHarness()
          return new Response(cached, { headers: { "content-type": `text/javascript; charset=utf-8`, "cache-control": `no-store` } })
        } catch (error) {
          return new Response(String(error), { status: 500 })
        }
      }
      return new Response(`Not found`, { status: 404 })
    },
  })
  console.log(`harness  http://localhost:${port}/?view=kitchen-sink`)
}
