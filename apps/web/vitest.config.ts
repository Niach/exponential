import { defineConfig } from "vitest/config"
import viteReact from "@vitejs/plugin-react"
import viteTsConfigPaths from "vite-tsconfig-paths"

export default defineConfig({
  plugins: [
    viteTsConfigPaths({
      projects: [`./tsconfig.json`],
    }),
    viteReact(),
  ],
  test: {
    environment: `jsdom`,
    globals: true,
    include: [`src/**/*.test.ts`, `src/**/*.test.tsx`],
    exclude: [`tests/e2e/**`],
  },
})
