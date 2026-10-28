import { defineConfig, type Plugin } from "vite"
import react from "@vitejs/plugin-react"
import { cpSync, existsSync, readFileSync, readdirSync, statSync } from "node:fs"
import { resolve } from "node:path"

/* The screenshot store (EXP-566) lives at the REPO ROOT, not in public/ — the
   same images feed the docs, the native parity lane and the store listings, so
   they can't belong to one app's public dir. This plugin is the whole bridge:
   the docs pages reference /shots/<view>/<platform>.webp, dev serves them from
   the store, and the build copies the store into dist. */
const SHOTS_DIR = resolve(__dirname, `../../shots`)

/* The blog (EXP-1366): blog/index.html plus one shell per post directory,
   found on disk so a new post needs no config change. */
const BLOG_DIR = resolve(__dirname, `blog`)
const blogInputs: Record<string, string> = Object.fromEntries([
  [`blog`, resolve(BLOG_DIR, `index.html`)],
  ...readdirSync(BLOG_DIR, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => [`blog-${entry.name}`, resolve(BLOG_DIR, entry.name, `index.html`)]),
])
const SHOTS_PREFIX = `/shots/`

function contentType(file: string): string {
  if (file.endsWith(`.webp`)) return `image/webp`
  if (file.endsWith(`.json`)) return `application/json`
  return `application/octet-stream`
}

/* The Exponential UI section (/ui/, formerly ui.exponential.at): ONE entry,
   ui/index.html, routing by path. In dev a page navigation under /ui/ (an
   HTML request, never a module or asset) gets that entry, so the guide
   example projects' own index.html under ui/guides/ never answers. */
function uiSection(): Plugin {
  return {
    name: `exp-ui-section`,
    configureServer(server) {
      server.middlewares.use((req, _res, next) => {
        const path = (req.url ?? ``).split(`?`)[0]
        const isPage = req.method === `GET` && (req.headers.accept ?? ``).includes(`text/html`)
        if (isPage && (path === `/ui` || path === `/talk` || ((path.startsWith(`/ui/`) || path.startsWith(`/talk/`)) && !/\.[a-z0-9]+$/i.test(path)))) req.url = `/ui/index.html`
        next()
      })
    },
  }
}

function shotStore(): Plugin {
  return {
    name: `exp-shot-store`,
    buildStart() {
      /* Creating it is the capture script's job, not the build's: a missing
         store means the checkout is wrong, and a silently-empty dist/shots
         would ship a docs page full of dark slabs. */
      if (!existsSync(SHOTS_DIR)) {
        throw new Error(`shots/ store missing — run bun run shots`)
      }
    },
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const path = (req.url ?? ``).split(`?`)[0]
        if (!path.startsWith(SHOTS_PREFIX)) return next()
        const file = resolve(
          SHOTS_DIR,
          decodeURIComponent(path.slice(SHOTS_PREFIX.length))
        )
        /* Path traversal: resolve first, then insist the result stays under
           the store. */
        if (!file.startsWith(`${SHOTS_DIR}/`)) {
          res.statusCode = 403
          res.end()
          return
        }
        if (!existsSync(file) || !statSync(file).isFile()) return next()
        res.setHeader(`Content-Type`, contentType(file))
        res.end(readFileSync(file))
      })
    },
    closeBundle() {
      /* Tolerates an empty store — shots are captured on their own lane and
         the site has to build before any of them exist. */
      if (!existsSync(SHOTS_DIR)) return
      cpSync(SHOTS_DIR, resolve(__dirname, `dist/shots`), { recursive: true })
    },
  }
}

export default defineConfig({
  plugins: [react(), uiSection(), shotStore()],
  resolve: {
    alias: {
      "@app": resolve(__dirname, `../web/src`),
    },
    // The @app alias reaches across workspaces, so any React copy nested
    // under apps/web would ride into a lazy chunk as a SECOND React instance
    // — its hooks dispatcher is null (EXP-207: a useId in a lazy chunk
    // crashed). Always bundle the one hoisted copy.
    dedupe: [`react`, `react-dom`],
  },
  /* Only the site's own pages: the dep scan must not crawl the guide example
     projects under ui/guides/ (Solid, the web component …). */
  optimizeDeps: {
    entries: [`index.html`, `*/index.html`, `docs/*/index.html`, `blog/*/index.html`],
  },
  build: {
    rollupOptions: {
      input: {
        home: resolve(__dirname, `index.html`),
        pricing: resolve(__dirname, `pricing/index.html`),
        download: resolve(__dirname, `download/index.html`),
        docs: resolve(__dirname, `docs/index.html`),
        "docs-getting-started": resolve(
          __dirname,
          `docs/getting-started/index.html`,
        ),
        "docs-issues": resolve(__dirname, `docs/issues/index.html`),
        "docs-coding": resolve(__dirname, `docs/coding/index.html`),
        "docs-actions": resolve(__dirname, `docs/actions/index.html`),
        "docs-cli": resolve(__dirname, `docs/cli/index.html`),
        "docs-feedback": resolve(__dirname, `docs/feedback/index.html`),
        "docs-widget": resolve(__dirname, `docs/widget/index.html`),
        "docs-mcp": resolve(__dirname, `docs/mcp/index.html`),
        "docs-apps": resolve(__dirname, `docs/apps/index.html`),
        "docs-selfhost": resolve(__dirname, `docs/self-host/index.html`),
        privacy: resolve(__dirname, `privacy/index.html`),
        terms: resolve(__dirname, `terms/index.html`),
        imprint: resolve(__dirname, `imprint/index.html`),
        contact: resolve(__dirname, `contact/index.html`),
        ui: resolve(__dirname, `ui/index.html`),
        ...blogInputs,
      },
    },
  },
})
