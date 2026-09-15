/**
 * Dev server. The shot STORE is re-read on every request and images are
 * streamed straight out of the repo, so a fresh capture shows up on reload
 * with no build step in between. The Components group is a frozen module
 * import (`COMPONENTS`, `renderHtml`): a component edit needs a restart.
 *
 * The `@exp/ui` stylesheet the islands wear is NOT recompiled per request — a
 * compile is ~1s, and editing a component changes its markup, not usually its
 * utilities. `?recompile=1` forces a fresh one when it does; a compile that
 * fails keeps the last good sheet and reports the error.
 */

import path from "node:path"

import { compileUiCss } from "@exp/ui/island"

import { COMPONENTS } from "./components.tsx"
import { renderHtml } from "./render.ts"
import { readGallery, storeDir } from "./store.ts"

const port = Number(process.env.PORT ?? 4173)

let uiCss = ``
let uiCssError: string | null = null

async function recompile(): Promise<void> {
  try {
    uiCss = await compileUiCss({ base: import.meta.dir })
    uiCssError = null
  } catch (error) {
    uiCssError = error instanceof Error ? error.message : String(error)
    console.error(`styleguide: ui css compile failed — ${uiCssError}`)
  }
}

await recompile()

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
      if (url.searchParams.get(`recompile`) === `1`) await recompile()
      if (uiCssError) {
        return new Response(`ui css compile failed:\n${uiCssError}`, {
          status: 500,
          headers: { "content-type": `text/plain; charset=utf-8` },
        })
      }
      return new Response(renderHtml(readGallery(root), COMPONENTS, uiCss), {
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
  error(error) {
    console.error(error)
    return new Response(`Internal error: ${error.message}`, { status: 500 })
  },
})

console.log(
  `styleguide  http://localhost:${server.port}  (store: ${storeDir()}, ui css ${Math.round(uiCss.length / 1024)} KB — ?recompile=1 rebuilds it)`
)
