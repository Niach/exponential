import { defineConfig } from "vitest/config"
import viteReact from "@vitejs/plugin-react"

// The DOM suites (fixtures → DOM snapshots, inputs, actions, extensions)
// run in jsdom; the browser suites (geometry, overlays, typing) are a
// separate config (`vitest.browser.config.ts`) because they drive headless
// Chromium through Playwright.
export default defineConfig({
  plugins: [viteReact()],
  test: {
    environment: `jsdom`,
    globals: true,
    include: [`src/**/*.test.ts`, `src/**/*.test.tsx`],
    setupFiles: [`./src/test-setup.ts`],
  },
})
