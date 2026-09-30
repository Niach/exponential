import { resolve } from "node:path"
import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import { VIEW_NAMES, type ViewName } from "./src/contract"

// EXP-1153: one IIFE bundle per view (`vite build --mode <view>`); Rollup's
// IIFE output takes a single entry, so scripts/build.ts runs one pass per
// view and then inlines each bundle into its HTML resource. An IIFE (not a
// module) because the host injects the document as `srcdoc` inside a
// sandboxed iframe, where a module script would be a needless failure class.
export default defineConfig(({ mode }) => {
  if (!(VIEW_NAMES as readonly string[]).includes(mode)) {
    throw new Error(`vite --mode must be one of ${VIEW_NAMES.join(`, `)}`)
  }
  const view = mode as ViewName
  return {
    plugins: [react()],
    resolve: {
      alias: [
        // Deep imports into @exp/ui (see tsconfig `paths`): the barrel drags
        // every component's dependency into the bundle (highlight.js, the
        // zod-backed domain, sonner…); a view names the files it uses.
        { find: /^@exp\/ui\/src\/(.*)$/, replacement: `${resolve(__dirname, `../ui/src`)}/$1` },
      ],
    },
    define: { "process.env.NODE_ENV": JSON.stringify(`production`) },
    build: {
      outDir: resolve(__dirname, `dist/js`),
      emptyOutDir: false,
      target: `es2020`,
      cssCodeSplit: false,
      lib: {
        entry: resolve(__dirname, `src/${view}.tsx`),
        name: `__expView`,
        formats: [`iife`],
        fileName: () => `${view}.js`,
      },
      rollupOptions: { output: { inlineDynamicImports: true } },
    },
    logLevel: `warn`,
  }
})
