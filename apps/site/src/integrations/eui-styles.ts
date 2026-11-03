/* eUI surfaces rendered at build time each inline the full theme sheet
   (BASE_CSS + the compiled theme, ~186 KB). This step moves every distinct
   sheet into ONE hashed file under /_astro/ (cached across pages), links it
   once in <head>, and leaves the surface's <style> empty. Hydration keeps
   the empty element (React never rewrites dangerouslySetInnerHTML when
   hydrating), so the islands keep working. */
import type { AstroIntegration } from "astro"
import { createHash } from "node:crypto"
import { mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { fileURLToPath } from "node:url"

const SHEET = /<style data-xui-style="theme">([\s\S]*?)<\/style>/g

function htmlFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) return htmlFiles(path)
    return name.endsWith(`.html`) ? [path] : []
  })
}

export function euiStyles(): AstroIntegration {
  return {
    name: `exp-eui-styles`,
    hooks: {
      "astro:build:done": ({ dir, logger }) => {
        const out = fileURLToPath(dir)
        const written = new Set<string>()
        let pages = 0
        for (const file of htmlFiles(out)) {
          const html = readFileSync(file, `utf8`)
          if (!html.includes(`data-xui-style="theme"`)) continue
          const links = new Set<string>()
          const next = html.replace(SHEET, (_, css: string) => {
            const name = `eui-theme.${createHash(`sha256`).update(css).digest(`hex`).slice(0, 10)}.css`
            if (!written.has(name)) {
              mkdirSync(join(out, `_astro`), { recursive: true })
              writeFileSync(join(out, `_astro`, name), css)
              written.add(name)
            }
            links.add(`<link rel="stylesheet" href="/_astro/${name}">`)
            return `<style data-xui-style="theme"></style>`
          })
          writeFileSync(file, next.replace(`</head>`, `${[...links].join(``)}</head>`))
          pages++
        }
        logger.info(`${written.size} eUI theme sheet(s) shared by ${pages} page(s)`)
      },
    },
  }
}
