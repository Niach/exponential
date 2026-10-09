#!/usr/bin/env bun
// The CONFORMANCE page server: bundles scripts/conformance-page.ts with Bun
// and serves it plus, under `/repo/<path>`, ONLY the repo files the matrix
// names (the manifest, its fixtures and data, the font manifest and its
// font files). `PORT` (default 4190). Spawned by browser/conformance.test.ts
// (vitest runs under Node) and started in-process by scripts/conformance-web.ts.

import { readFileSync } from "node:fs"
import { join } from "node:path"
import { fontFiles, MANIFEST_PATH, type ConformanceManifest, type FontManifest } from "../../exponential-ui/conformance/dump"

const pkgRoot = join(import.meta.dir, `..`)
export const repoRoot = join(pkgRoot, `..`, `..`)

const PAGE = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Exponential UI · conformance</title></head><body><div id="app"></div><script type="module" src="/dist/page.js"></script></body></html>`

/** The repo-relative files the page may read. */
export function allowedFiles(): Set<string> {
  const manifest = JSON.parse(readFileSync(join(repoRoot, MANIFEST_PATH), `utf8`)) as ConformanceManifest
  const fonts = JSON.parse(readFileSync(join(repoRoot, manifest.fonts), `utf8`)) as FontManifest
  const files = new Set<string>([MANIFEST_PATH, manifest.fonts, ...fontFiles(fonts)])
  for (const f of Object.values(manifest.fixtures)) for (const p of [f.tree, f.data, f.geometry]) if (p) files.add(p)
  return files
}

async function bundle(): Promise<string> {
  const result = await Bun.build({ entrypoints: [join(import.meta.dir, `conformance-page.ts`)], target: `browser`, format: `esm`, minify: false, define: { "process.env.NODE_ENV": JSON.stringify(`production`) } })
  if (!result.success) throw new Error(result.logs.map((l) => l.message).join(`\n`))
  return await result.outputs[0].text()
}

export function startConformanceServer(port: number): { url: string; stop: () => void } {
  const allowed = allowedFiles()
  let cached: Promise<string> | null = null
  const server = Bun.serve({
    port,
    hostname: `127.0.0.1`,
    async fetch(req) {
      const url = new URL(req.url)
      if (url.pathname === `/` || url.pathname === `/index.html`) return new Response(PAGE, { headers: { "content-type": `text/html; charset=utf-8` } })
      if (url.pathname === `/dist/page.js`) {
        try {
          cached ??= bundle()
          return new Response(await cached, { headers: { "content-type": `text/javascript; charset=utf-8`, "cache-control": `no-store` } })
        } catch (error) {
          cached = null
          return new Response(String(error), { status: 500 })
        }
      }
      if (url.pathname.startsWith(`/repo/`)) {
        const rel = decodeURIComponent(url.pathname.slice(`/repo/`.length))
        if (!allowed.has(rel)) return new Response(`Not in the conformance allowlist`, { status: 403 })
        const type = rel.endsWith(`.json`) ? `application/json` : rel.endsWith(`.ttf`) ? `font/ttf` : `application/octet-stream`
        return new Response(Bun.file(join(repoRoot, rel)), { headers: { "content-type": type } })
      }
      return new Response(`Not found`, { status: 404 })
    },
  })
  return { url: `http://127.0.0.1:${server.port}`, stop: () => server.stop(true) }
}

if (import.meta.main) {
  const { url } = startConformanceServer(Number(process.env.PORT ?? 4190))
  console.log(`conformance  ${url}/?case=kitchen-sink/exponential/dark/900/ltr`)
}
