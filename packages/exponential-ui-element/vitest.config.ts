import { defineConfig } from "vitest/config"
import solid from "vite-plugin-solid"

// FEED-130: the element's DOM suites run in jsdom (customElements, the
// Solid painter's client build). The package itself has no JSX; the Solid
// plugin is here because `@exponential_app/ui-solid` resolves to its TSX
// sources inside the monorepo. The React renderer (a dev dependency, the
// DOM-parity reference) stays out of the Solid JSX transform.
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
