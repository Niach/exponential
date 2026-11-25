import { fileURLToPath } from "node:url"
import tailwindcss from "@tailwindcss/vite"
import viteReact from "@vitejs/plugin-react"
import { defineConfig, type Plugin } from "vite"
import { pruneUtilities } from "./src/build/css"
import { MCP_APP_MAX_BYTES, leanJson, packedText } from "./src/build/lean"

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

// #NEW-140: the Motion wasm. `genui` carries it gzipped (428 kB less than the
// bytes as a data: URL, and no fetch: the sandbox's `connect-src` refuses a
// data: URL too); the other views draw Markdown and lists, never a Motion.
const MOTION_WASM = fileURLToPath(new URL(ENTRY === `genui` ? `./src/genui/motion-wasm.ts` : `./src/build/no-motion-wasm.ts`, import.meta.url))

function leanMotionWasm(): Plugin {
  return {
    name: `exp-mcp-apps-motion-wasm`,
    enforce: `pre`,
    resolveId: (source, importer) => (/^(?:\.\.\/)+motion\/wasm$/.test(source) && importer?.includes(`/packages/exponential-ui/src/`) ? MOTION_WASM : null),
    // The glue's own fetch fallback names the .wasm, which would be inlined.
    transform: (code, id) => (id.endsWith(`/eui_motion_wasm.js`) ? { code: code.replace(/new URL\([^)]*import\.meta\.url\)/, `undefined`), map: null } : null),
  }
}

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
      const script = Object.values(bundle).map((file) => (file.type === `chunk` ? file.code : ``)).join(`\n`) + packedText()
      for (const [name, file] of Object.entries(bundle)) {
        if (file.type === `chunk` && file.isEntry) {
          // One chunk has nothing to preload; Vite fills its marker in after this
          // hook, and a lazy import (the Motion engine) would throw on it.
          const code = file.code.replaceAll(`__VITE_PRELOAD__`, `void 0`).replace(/<\/script/gi, `<\\/script`)
          source = source.replace(
            new RegExp(`<script[^>]*src="[^"]*${escape(file.fileName)}"[^>]*></script>`),
            () => `<script type="module">${code}</script>`
          )
          delete bundle[name]
        } else if (file.type === `asset` && file.fileName.endsWith(`.css`)) {
          const css = pruneUtilities(String(file.source), script).replace(/<\/style/gi, `<\\/style`)
          source = source.replace(
            new RegExp(`<link[^>]*href="[^"]*${escape(file.fileName)}"[^>]*>`),
            () => `<style>${css}</style>`
          )
          delete bundle[name]
        }
      }
      // #NEW-140: the size budget. OpenClaw caps a `ui://` resource; a view
      // over its own fails the build (the gate's web production build runs it).
      const bytes = Buffer.byteLength(source)
      if (bytes > MCP_APP_MAX_BYTES[ENTRY]) throw new Error(`${ENTRY}.html is ${bytes} bytes, over its budget of ${MCP_APP_MAX_BYTES[ENTRY]} (src/build/lean.ts; OpenClaw caps a resource at 2 MiB).`)
      html.source = source
      html.fileName = `${ENTRY}.html`
    },
  }
}

function escape(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, `\\$&`)
}

export default defineConfig({
  plugins: [leanMotionWasm(), ...leanJson(), viteReact(), tailwindcss(), inlineIntoHtml()],
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
