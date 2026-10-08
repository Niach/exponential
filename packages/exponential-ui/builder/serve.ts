#!/usr/bin/env bun
// VAPP-92: `bun run --filter @exponential-at/ui dev:builder` — bundles the
// builder page on every request and serves it on :4180.

import { join } from "node:path"

const root = import.meta.dir
const port = Number(process.env.PORT ?? 4180)

async function bundle(): Promise<string> {
  const result = await Bun.build({ entrypoints: [join(root, `main.ts`)], target: `browser`, format: `esm`, minify: false, define: { "process.env.NODE_ENV": JSON.stringify(`production`) } })
  if (!result.success) throw new Error(result.logs.map((l) => l.message).join(`\n`))
  return await result.outputs[0].text()
}

Bun.serve({
  port,
  async fetch(req) {
    const url = new URL(req.url)
    if (url.pathname === `/` || url.pathname === `/index.html`) return new Response(Bun.file(join(root, `index.html`)), { headers: { "content-type": `text/html; charset=utf-8` } })
    if (url.pathname === `/builder.css`) return new Response(Bun.file(join(root, `builder.css`)), { headers: { "content-type": `text/css` } })
    if (url.pathname === `/dist/main.js`) {
      try {
        return new Response(await bundle(), { headers: { "content-type": `text/javascript; charset=utf-8`, "cache-control": `no-store` } })
      } catch (error) {
        return new Response(String(error), { status: 500 })
      }
    }
    return new Response(`Not found`, { status: 404 })
  },
})
console.log(`theme builder  http://localhost:${port}`)
