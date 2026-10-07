import { fileURLToPath } from "node:url"
import tailwindcss from "@tailwindcss/vite"
import viteReact from "@vitejs/plugin-react"
import { defineConfig, type Plugin } from "vite"

// EXP-1183 — the MCP Apps views ship as ONE self-contained HTML document: an
// MCP Apps host (OpenClaw, Claude, ChatGPT…) reads it as a `ui://` resource
// and mounts it in a sandboxed iframe with no network, so the script and the
// stylesheet must ride inline. The web server serves the file as every
// `ui://exponential/*` resource (apps/web/src/lib/mcp/apps.ts), stamping the
// view name into it; like the widget it is built into the web public dir
// BEFORE the web build, which copies it into `.output/public`.
const OUT_DIR = fileURLToPath(
  new URL(`../../apps/web/public/mcp-apps`, import.meta.url)
)

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
      html.fileName = `app.html`
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
    emptyOutDir: true,
    cssCodeSplit: false,
    assetsInlineLimit: Number.MAX_SAFE_INTEGER,
    modulePreload: false,
    rollupOptions: {
      output: { inlineDynamicImports: true },
    },
  },
})
