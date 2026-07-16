import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import { resolve } from "node:path"

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      // The site self-hosts Inter/JetBrains Mono (fonts.css) — the video
      // surfaces must never fetch from Google. Keep these two exact shims in
      // lockstep with tsconfig "paths" (and extend both if apps/video ever
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
      "@video": resolve(__dirname, `../video/src`),
    },
  },
  build: {
    rollupOptions: {
      input: {
        home: resolve(__dirname, `index.html`),
        pricing: resolve(__dirname, `pricing/index.html`),
        download: resolve(__dirname, `download/index.html`),
        docs: resolve(__dirname, `docs/index.html`),
        "docs-selfhost": resolve(__dirname, `docs/self-host/index.html`),
        privacy: resolve(__dirname, `privacy/index.html`),
        terms: resolve(__dirname, `terms/index.html`),
        imprint: resolve(__dirname, `imprint/index.html`),
        contact: resolve(__dirname, `contact/index.html`),
      },
    },
  },
})
