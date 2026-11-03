/* The repo's screenshot store (shots/<view>/<platform>.webp) is the site's
   picture source. Dev serves it from the store; the build copies only the
   shots the built pages reference, so dist stays small. */
import type { AstroIntegration } from "astro"
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const SHOTS = resolve(dirname(fileURLToPath(import.meta.url)), `../../../../shots`)
const REF = /\/shots\/([a-z0-9-]+\/[a-z0-9-]+\.webp)/g

function htmlFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) return htmlFiles(path)
    return name.endsWith(`.html`) ? [path] : []
  })
}

export function shotStore(): AstroIntegration {
  return {
    name: `exp-shot-store`,
    hooks: {
      "astro:config:setup": ({ updateConfig }) => {
        if (!existsSync(SHOTS)) throw new Error(`shots/ store missing at ${SHOTS}`)
        updateConfig({
          vite: {
            plugins: [
              {
                name: `exp-shot-store-dev`,
                configureServer(server) {
                  server.middlewares.use((req, res, next) => {
                    const m = /^\/shots\/([a-z0-9-]+\/[a-z0-9-]+\.webp)$/.exec((req.url ?? ``).split(`?`)[0]!)
                    const file = m ? join(SHOTS, m[1]!) : ``
                    if (!m || !existsSync(file)) return next()
                    res.setHeader(`Content-Type`, `image/webp`)
                    res.end(readFileSync(file))
                  })
                },
              },
            ],
          },
        })
      },
      "astro:build:done": ({ dir, logger }) => {
        const out = fileURLToPath(dir)
        const wanted = new Set<string>()
        for (const file of htmlFiles(out)) for (const m of readFileSync(file, `utf8`).matchAll(REF)) wanted.add(m[1]!)
        for (const rel of wanted) {
          const from = join(SHOTS, rel)
          if (!existsSync(from)) throw new Error(`page references a missing shot: shots/${rel}`)
          mkdirSync(dirname(join(out, `shots`, rel)), { recursive: true })
          copyFileSync(from, join(out, `shots`, rel))
        }
        logger.info(`copied ${wanted.size} shots`)
      },
    },
  }
}
