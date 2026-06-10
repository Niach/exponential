import { copyFileSync, mkdirSync } from "node:fs"
import { resolve } from "node:path"
import { defineConfig, type Plugin } from "vite"
import preact from "@preact/preset-vite"

// Both artifacts build straight into the web app's public dir (gitignored)
// so Nitro serves them in dev and copies them into .output/public for prod.
const outDir = resolve(__dirname, `../../apps/web/public/widget/v1`)

// Two-pass build (`vite build` then `vite build --mode loader`): Rollup's
// IIFE output does not support multiple entries in one pass. IIFE (classic
// scripts, not ESM) is deliberate — module scripts are always fetched in CORS
// mode on third-party pages, which is an avoidable failure class.
function copyDemoPage(): Plugin {
  return {
    name: `exp-widget-copy-demo`,
    closeBundle() {
      mkdirSync(outDir, { recursive: true })
      copyFileSync(
        resolve(__dirname, `src/demo/demo.html`),
        resolve(outDir, `demo.html`)
      )
    },
  }
}

export default defineConfig(({ mode }) => {
  const isLoader = mode === `loader`
  return {
    plugins: isLoader ? [] : [preact(), copyDemoPage()],
    build: {
      outDir,
      // The widget pass runs first and cleans; the loader pass appends.
      emptyOutDir: !isLoader,
      target: `es2019`,
      lib: {
        entry: resolve(__dirname, isLoader ? `src/loader.ts` : `src/main.ts`),
        name: isLoader ? `__expWidgetLoader` : `__expWidgetBundle`,
        formats: [`iife`],
        fileName: () => (isLoader ? `loader.js` : `widget.js`),
      },
      rollupOptions: { output: { inlineDynamicImports: true } },
    },
    test: {
      environment: `happy-dom`,
      environmentOptions: {
        happyDOM: {
          // Injected <script src> must stay pending like a real network
          // load, not fail synchronously (loader.test.ts relies on this).
          settings: {
            disableJavaScriptFileLoading: true,
            disableCSSFileLoading: true,
            handleDisabledFileLoadingAsSuccess: true,
          },
        },
      },
      include: [`src/**/*.test.ts`],
    },
  }
})
