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
    // `scripts/` is in here for the one-off backfills: they touch prod data
    // once, from a laptop, with nobody watching the guards but the person
    // running them, so their rules belong under CI like everything else.
    include: [
      `src/**/*.test.ts`,
      `src/**/*.test.tsx`,
      `scripts/**/*.test.ts`,
    ],
    exclude: [`tests/e2e/**`],
  },
})
