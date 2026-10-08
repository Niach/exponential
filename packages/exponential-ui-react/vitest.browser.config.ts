import { defineConfig } from "vitest/config"

// Headless-Chromium suites (`bun run --filter @exponential-at/ui-react
// test:browser`): each test bundles the harness with Bun, serves it on a
// loopback port and drives it with Playwright. They need the Playwright
// Chromium build on the machine (`bunx playwright install chromium`).
export default defineConfig({
  test: {
    environment: `node`,
    globals: true,
    include: [`browser/**/*.test.ts`],
    testTimeout: 120_000,
    hookTimeout: 120_000,
    fileParallelism: false,
  },
})
