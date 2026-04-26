import { defineConfig, type PluginOption } from "vite"
import { devtools } from "@tanstack/devtools-vite"
import { tanstackStart } from "@tanstack/react-start/plugin/vite"
import viteReact from "@vitejs/plugin-react"
import viteTsConfigPaths from "vite-tsconfig-paths"
import tailwindcss from "@tailwindcss/vite"
import { nitro } from "nitro/vite"

const plugins: PluginOption[] = [
  ...(process.env.DISABLE_TANSTACK_DEVTOOLS === `1` ? [] : [devtools()]),
  // Custom server entry adds Bun.serve idleTimeout: 255 (default is 10s,
  // which kills Electric long-poll connections mid-flight). See src/server-bun.ts.
  nitro({ entry: `./src/server-bun.ts` }),
  viteTsConfigPaths({
    projects: [`./tsconfig.json`],
  }),
  tailwindcss(),
  tanstackStart(),
  viteReact(),
]

const config = defineConfig({
  plugins,
  server: {
    port: 5173,
    host: true,
    allowedHosts: [`localhost`],
  },
  optimizeDeps: {
    exclude: [`@tanstack/start-server-core`],
  },
  ssr: {
    noExternal: [`zod`, `drizzle-orm`],
  },
  test: {
    environment: `jsdom`,
    globals: true,
  },
})

export default config
