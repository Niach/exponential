/* dist/serve.json for the static server: serve.base.json's headers with
   the CSP's script-src listing the sha256 of every inline script Astro and
   Starlight emit (theme, islands), plus the 301s of src/redirects.ts.
   'wasm-unsafe-eval' lets Motion's engine instantiate (the old site's
   blank Motion tiles: the CSP refused WebAssembly). */
import type { AstroIntegration } from "astro"
import { createHash } from "node:crypto"
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs"
import { join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { REDIRECTS } from "../redirects.ts"

const BASE = resolve(fileURLToPath(import.meta.url), `../../../serve.base.json`)
const INLINE = /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g

function htmlFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) return htmlFiles(path)
    return name.endsWith(`.html`) ? [path] : []
  })
}

export function serveConfig(): AstroIntegration {
  return {
    name: `exp-serve-config`,
    hooks: {
      "astro:build:done": ({ dir, logger }) => {
        const out = fileURLToPath(dir)
        const hashes = new Set<string>()
        for (const file of htmlFiles(out)) {
          for (const m of readFileSync(file, `utf8`).matchAll(INLINE)) {
            if (/type="application\/(?:ld\+)?json"/.test(m[0])) continue
            if (!m[1]!.trim()) continue
            hashes.add(`'sha256-${createHash(`sha256`).update(m[1]!).digest(`base64`)}'`)
          }
        }
        const base = readFileSync(BASE, `utf8`).replace(`{{SCRIPT_HASHES}}`, [...hashes].sort().join(` `))
        const config = JSON.parse(base) as { $comment?: string; redirects: { source: string; destination: string; type: number }[] }
        delete config.$comment
        config.redirects = Object.entries(REDIRECTS).map(([source, destination]) => ({ source, destination, type: 301 }))
        writeFileSync(join(out, `serve.json`), `${JSON.stringify(config, null, 2)}\n`)
        logger.info(`serve.json: ${hashes.size} inline script hashes, ${config.redirects.length} redirects`)
      },
    },
  }
}
