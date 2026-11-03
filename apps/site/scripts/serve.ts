/* Serves dist/ the way production does: dist/serve.json's headers (the
   CSP included) and 301s, directory index.html, trailing slashes. For the
   paint check and local previews: `bun run scripts/serve.ts [port]`. */
import { existsSync, readFileSync, statSync } from "node:fs"
import { join, resolve } from "node:path"
import { redirectFor } from "../src/redirects.ts"

const DIST = resolve(import.meta.dir, `../dist`)
const config = JSON.parse(readFileSync(join(DIST, `serve.json`), `utf8`)) as {
  headers: { source: string; headers: { key: string; value: string }[] }[]
}
const global = Object.fromEntries(config.headers.find((h) => h.source === `**`)!.headers.map((h) => [h.key, h.value]))
const port = Number(process.argv[2] ?? 4321)

Bun.serve({
  port,
  fetch(req) {
    const path = decodeURIComponent(new URL(req.url).pathname)
    const to = redirectFor(path)
    if (to) return new Response(null, { status: 301, headers: { Location: to } })
    let file = join(DIST, path)
    if (!file.startsWith(DIST)) return new Response(`forbidden`, { status: 403 })
    if (existsSync(file) && statSync(file).isDirectory()) {
      if (!path.endsWith(`/`)) return new Response(null, { status: 301, headers: { Location: `${path}/` } })
      file = join(file, `index.html`)
    }
    if (!existsSync(file)) return new Response(Bun.file(join(DIST, `404.html`)), { status: 404, headers: { ...global, "Content-Type": `text/html` } })
    return new Response(Bun.file(file), { headers: global })
  },
})
console.log(`dist/ on http://localhost:${port}`)
