import { defineConfig } from "vitest/config"
import viteReact from "@vitejs/plugin-react"

// The package owns no `@/` alias at runtime (every import in src/ is
// relative), so unlike apps/web this config needs no tsconfig-paths plugin.
export default defineConfig({
  plugins: [viteReact()],
  test: {
    environment: `jsdom`,
    globals: true,
    include: [`src/**/*.test.ts`, `src/**/*.test.tsx`],
  },
})
