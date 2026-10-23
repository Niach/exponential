import { defineConfig } from "vitest/config"
import solid from "vite-plugin-solid"

// Headless-Chromium suites (`bun run --filter @exponential-at/ui-solid
// test:browser`): each test builds the harness with Vite, serves it on a
// loopback port and drives it with Playwright. They need the Playwright
// Chromium build on the machine (`bunx playwright install chromium`).
// suites.test.ts imports Solid's own logic (src/node-view.tsx), hence
// Solid's JSX transform here and its `@vitest-environment jsdom` (solid-js's
// client build delegates events on `window` at module load).
export default defineConfig({
  plugins: [solid()],
  test: {
    environment: `node`,
    globals: true,
    include: [`browser/**/*.test.ts`],
    testTimeout: 120_000,
    hookTimeout: 120_000,
    fileParallelism: false,
  },
})
