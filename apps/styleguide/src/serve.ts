/**
 * Dev server. The page is re-rendered from disk on every request and images are
 * streamed straight out of the repo store, so a fresh capture shows up on
 * reload with no build step in between.
 */

import path from "node:path"

import { renderHtml } from "./render.ts"
import { readGallery, storeDir } from "./store.ts"

const port = Number(process.env.PORT ?? 4173)

/** Resolve inside the store, or `undefined` for anything that escapes it. */
function resolveInStore(root: string, urlPath: string): string | undefined {
  let decoded: string
  try {
    decoded = decodeURIComponent(urlPath)
  } catch {
    return undefined
  }
  const resolved = path.resolve(root, `.${decoded}`)
  if (resolved !== root && !resolved.startsWith(root + path.sep)) return undefined
  return resolved
}

const server = Bun.serve({
  port,
  async fetch(request) {
    const url = new URL(request.url)
    const root = storeDir()

    if (url.pathname === `/` || url.pathname === `/index.html`) {
      return new Response(renderHtml(readGallery(root)), {
        headers: { "content-type": `text/html; charset=utf-8`, "cache-control": `no-store` },
      })
    }

    if (url.pathname.startsWith(`/shots/`)) {
      const file = resolveInStore(root, url.pathname.slice(`/shots`.length))
      if (file === undefined) return new Response(`Forbidden`, { status: 403 })
      const blob = Bun.file(file)
      if (!(await blob.exists())) return new Response(`Not found`, { status: 404 })
      return new Response(blob, { headers: { "cache-control": `no-store` } })
    }

    return new Response(`Not found`, { status: 404 })
  },
})

console.log(`styleguide  http://localhost:${server.port}  (store: ${storeDir()})`)
