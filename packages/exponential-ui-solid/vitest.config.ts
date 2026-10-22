import { defineConfig } from "vitest/config"
import solid from "vite-plugin-solid"

// The DOM suites (fixtures → DOM snapshots, inputs, actions, extensions)
// run in jsdom on `@solidjs/testing-library`; the browser suites
// (`vitest.browser.config.ts`) drive headless Chromium through Playwright.
// The parity suite (src/parity.test.ts) also renders the React renderer
// (a dev dependency): its `.tsx` stays out of the Solid JSX transform and
// compiles with its own tsconfig (`jsx: react-jsx`).
export default defineConfig({
  plugins: [solid({ exclude: [/packages\/exponential-ui-react\//, /node_modules\/(react|react-dom|radix-ui|@radix-ui)\//] })],
  resolve: { conditions: [`development`, `browser`] },
  test: {
    environment: `jsdom`,
    globals: true,
    include: [`src/**/*.test.ts`, `src/**/*.test.tsx`],
    setupFiles: [`./src/test-setup.ts`],
  },
})
