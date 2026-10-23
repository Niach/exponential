import { defineConfig } from "vitest/config"
import solid from "vite-plugin-solid"

// FEED-130: the headless-Chromium gates (`test:conformance`, `test:parity`):
// each builds the element harness with Vite, serves it on a loopback port
// and drives it with Playwright (needs `bunx playwright install chromium`,
// or EXPONENTIAL_UI_CHROME). suites.test.ts imports the Solid renderer's own
// logic, hence the Solid plugin and jsdom for that file.
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
