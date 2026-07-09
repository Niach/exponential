import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import { resolve } from "node:path"

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@app": resolve(__dirname, `../web/src`),
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
