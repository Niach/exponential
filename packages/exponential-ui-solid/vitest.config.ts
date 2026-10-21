import { defineConfig } from "vitest/config"
import solid from "vite-plugin-solid"

// The DOM suites (fixtures → DOM snapshots, inputs, actions, extensions)
// run in jsdom on `@solidjs/testing-library`; the browser suites
// (`vitest.browser.config.ts`) drive headless Chromium through Playwright.
export default defineConfig({
  plugins: [solid()],
  resolve: { conditions: [`development`, `browser`] },
  test: {
    environment: `jsdom`,
    globals: true,
    include: [`src/**/*.test.ts`, `src/**/*.test.tsx`],
    setupFiles: [`./src/test-setup.ts`],
  },
})
