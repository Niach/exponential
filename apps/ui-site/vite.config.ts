import { defineConfig, type Plugin } from "vite"
import react from "@vitejs/plugin-react"
import { cpSync, existsSync, readFileSync, readdirSync, statSync } from "node:fs"
import { resolve } from "node:path"

/* Two stores this site reads but does not own, served in dev and copied into
   dist at build time (shared, never copied into the repo):
   - the brand assets + self-hosted fonts of exponential.at
     (apps/marketing/public: fonts/, favicon, logos, touch icons);
   - the screenshot store at the repo root (EXP-566), only its Exponential UI
     views (`exponential-ui-*`: the specimens + the kitchen sink), which the
     component pages show as the four platform shots. */
const MARKETING_PUBLIC = resolve(__dirname, `../marketing/public`)
const SHARED_ASSETS = [`fonts`, `favicon.ico`, `logo-light.svg`, `logo-dark.svg`, `apple-touch-icon.png`, `icon-192.png`, `icon-512.png`]
const SHOTS_DIR = resolve(__dirname, `../../shots`)
const SHOT_VIEW_PREFIX = `exponential-ui-`

function contentType(file: string): string {
  if (file.endsWith(`.webp`)) return `image/webp`
  if (file.endsWith(`.woff2`)) return `font/woff2`
  if (file.endsWith(`.svg`)) return `image/svg+xml`
  if (file.endsWith(`.png`)) return `image/png`
  if (file.endsWith(`.ico`)) return `image/x-icon`
  return `application/octet-stream`
}

/** `url` → a file under `root`, or null (path traversal refused). */
function fileUnder(root: string, rel: string): string | null {
  const file = resolve(root, decodeURIComponent(rel))
  if (!file.startsWith(`${root}/`)) return null
  return existsSync(file) && statSync(file).isFile() ? file : null
}

function sharedStores(): Plugin {
  return {
    name: `ui-site-shared-stores`,
    buildStart() {
      if (!existsSync(SHOTS_DIR)) throw new Error(`shots/ store missing — run bun run shots`)
    },
    configureServer(server) {
      /* The site is ONE entry routing by path: a page navigation (an HTML
         request, never a module or asset) always gets index.html, so the
         guide example projects' own index.html under guides/ never answers
         /guides/<slug>/. */
      server.middlewares.use((req, _res, next) => {
        const path = (req.url ?? ``).split(`?`)[0]
        const isPage = req.method === `GET` && (req.headers.accept ?? ``).includes(`text/html`) && path.endsWith(`/`)
        if (isPage) req.url = `/`
        next()
      })
      server.middlewares.use((req, res, next) => {
        const path = (req.url ?? ``).split(`?`)[0]
        let file: string | null = null
        if (path.startsWith(`/shots/${SHOT_VIEW_PREFIX}`)) file = fileUnder(SHOTS_DIR, path.slice(`/shots/`.length))
        else if (SHARED_ASSETS.some((a) => path === `/${a}` || path.startsWith(`/${a}/`))) file = fileUnder(MARKETING_PUBLIC, path.slice(1))
        if (!file) return next()
        res.setHeader(`Content-Type`, contentType(file))
        res.end(readFileSync(file))
      })
    },
    closeBundle() {
      const dist = resolve(__dirname, `dist`)
      for (const asset of SHARED_ASSETS) cpSync(resolve(MARKETING_PUBLIC, asset), resolve(dist, asset), { recursive: true })
      for (const view of readdirSync(SHOTS_DIR)) {
        if (view.startsWith(SHOT_VIEW_PREFIX)) cpSync(resolve(SHOTS_DIR, view), resolve(dist, `shots`, view), { recursive: true })
      }
    },
  }
}

export default defineConfig({
  plugins: [react(), sharedStores()],
  resolve: {
    dedupe: [`react`, `react-dom`],
  },
  build: {
    rollupOptions: {
      input: { main: resolve(__dirname, `index.html`) },
    },
  },
})
