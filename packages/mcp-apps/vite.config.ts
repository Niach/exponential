import { fileURLToPath } from "node:url"
import tailwindcss from "@tailwindcss/vite"
import viteReact from "@vitejs/plugin-react"
import { defineConfig, type Plugin } from "vite"

// EXP-1183 — the MCP Apps views ship as self-contained HTML documents: an
// MCP Apps host (OpenClaw, Claude, ChatGPT…) reads one as a `ui://` resource
// and mounts it in a sandboxed iframe with no network, so the script and the
// stylesheet must ride inline. The web server serves each `ui://exponential/*`
// resource from its own file (apps/web/src/lib/emcp/apps.ts); like the widget
// they are built into the web public dir BEFORE the web build, which copies
// them into `.output/public`.
const OUT_DIR = fileURLToPath(
  new URL(`../../apps/web/public/mcp-apps`, import.meta.url)
)

// #NEW-24: ONE document per view (`issues.html`, `inbox.html`, `devices.html`,
// and #VAPP-292's `genui.html`: the eUI renderer + the app host), one pass
// each (`MCP_APPS_ENTRY=<view>`, the package's `build` script runs the four),
// so a view carries only its own code and the documents share nothing at
// runtime. Each pass inlines its one entry.
const ENTRIES = [`issues`, `inbox`, `devices`, `genui`] as const
const ENTRY = ENTRIES.find((entry) => entry === process.env.MCP_APPS_ENTRY) ?? ENTRIES[0]

// #NEW-24: the workspace packages name no `sideEffects`, so every module a
// barrel (`@exp/ui`) re-exports rode along whether a view drew with it or
// not: the catalogue's specimens, the fixtures, the Motion wasm glue. For
// THESE documents their modules are pure (a stylesheet never is): what a
// view does not import is dropped.
const PURE_MODULES = /\/packages\/(?:ui|exponential-ui|exponential-ui-react|domain-contract|etalk|icons|emoji|db-schema|mcp-apps\/src\/(?!entries\/|genui\/main|mount))[^?]*\.(?:tsx?|json|js)$/

/** Folds the emitted JS + CSS chunks into the HTML and drops them. */
function inlineIntoHtml(): Plugin {
  return {
    name: `exp-mcp-apps-inline`,
    enforce: `post`,
    generateBundle(_options, bundle) {
      const html = Object.values(bundle).find(
        (file) => file.type === `asset` && file.fileName.endsWith(`.html`)
      )
      if (!html || html.type !== `asset`) return
      let source = String(html.source)
      for (const [name, file] of Object.entries(bundle)) {
        if (file.type === `chunk` && file.isEntry) {
          const code = file.code.replace(/<\/script/gi, `<\\/script`)
          source = source.replace(
            new RegExp(`<script[^>]*src="[^"]*${escape(file.fileName)}"[^>]*></script>`),
            () => `<script type="module">${code}</script>`
          )
          delete bundle[name]
        } else if (file.type === `asset` && file.fileName.endsWith(`.css`)) {
          const css = String(file.source).replace(/<\/style/gi, `<\\/style`)
          source = source.replace(
            new RegExp(`<link[^>]*href="[^"]*${escape(file.fileName)}"[^>]*>`),
            () => `<style>${css}</style>`
          )
          delete bundle[name]
        }
      }
      html.source = source
      html.fileName = `${ENTRY}.html`
    },
  }
}

function escape(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, `\\$&`)
}

export default defineConfig({
  plugins: [viteReact(), tailwindcss(), inlineIntoHtml()],
  base: `./`,
  build: {
    outDir: OUT_DIR,
    // The first pass empties the directory; the others keep its documents.
    emptyOutDir: ENTRY === ENTRIES[0],
    cssCodeSplit: false,
    assetsInlineLimit: Number.MAX_SAFE_INTEGER,
    modulePreload: false,
    rolldownOptions: {
      input: fileURLToPath(new URL(`./${ENTRY}.html`, import.meta.url)),
      output: { codeSplitting: false },
      treeshake: { moduleSideEffects: (id) => !PURE_MODULES.test(id) },
    },
  },
})
