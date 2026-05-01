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
        privacy: resolve(__dirname, `privacy/index.html`),
        terms: resolve(__dirname, `terms/index.html`),
      },
    },
  },
})
