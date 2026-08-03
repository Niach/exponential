import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import { resolve } from "node:path"

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      // The site self-hosts Inter/JetBrains Mono (fonts.css) — the movie
      // surfaces must never fetch from Google. Keep these two exact shims in
      // lockstep with tsconfig "paths" (and extend both if src/movie ever
      // imports another @remotion/google-fonts family).
      "@remotion/google-fonts/Inter": resolve(
        __dirname,
        `src/movie/fonts/inter-shim.ts`,
      ),
      "@remotion/google-fonts/JetBrainsMono": resolve(
        __dirname,
        `src/movie/fonts/jetbrains-shim.ts`,
      ),
      "@app": resolve(__dirname, `../web/src`),
    },
    // The @app alias reaches across workspaces, so any React copy nested
    // under apps/web would ride into a lazy chunk as a SECOND React instance
    // — its hooks dispatcher is null (EXP-207: the movie ending slide's
    // useId crashed the player). Always bundle the one hoisted copy.
    dedupe: [`react`, `react-dom`],
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
      },
    },
  },
})
