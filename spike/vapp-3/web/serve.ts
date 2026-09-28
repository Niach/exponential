// VAPP-3 spike web host: serves the bench page, hands it the dev tickets, appends results.
//   bun spike/vapp-3/web/serve.ts            → http://localhost:8787/?policy=all&runs=20&auto=1
import { appendFileSync, readFileSync } from "node:fs"
import { join } from "node:path"

const here = new URL(".", import.meta.url).pathname
const results = join(here, "..", "results")
const out = process.env.VAPP3_WEB_OUT ?? join(results, "a-web.jsonl")
const port = Number(process.env.PORT ?? 8787)

Bun.serve({
  port,
  hostname: "0.0.0.0",
  async fetch(req) {
    const url = new URL(req.url)
    if (req.method === "POST" && url.pathname === "/results") {
      const body = await req.json()
      appendFileSync(out, `${JSON.stringify(body)}\n`)
      return new Response("ok")
    }
    if (url.pathname === "/tickets") {
      const name = url.searchParams.get("room") ?? "dev"
      if (!/^[\w-]+$/.test(name)) return new Response("bad room", { status: 400 })
      return new Response(readFileSync(join(results, `.tickets-${name}.json`)), { headers: { "content-type": "application/json", "cache-control": "no-store" } })
    }
    const file = url.pathname === "/" ? "index.html" : url.pathname.slice(1)
    if (!["index.html", "peer.js"].includes(file)) return new Response("not found", { status: 404 })
    return new Response(Bun.file(join(here, file)), {
      headers: { "content-type": file.endsWith(".js") ? "text/javascript" : "text/html", "cache-control": "no-store" },
    })
  },
})
console.log(`vapp3 web on http://localhost:${port}/  (results → ${out})`)
