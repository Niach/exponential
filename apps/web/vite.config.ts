import { readFileSync } from "node:fs"
import { defineConfig, type PluginOption } from "vite"
import { devtools } from "@tanstack/devtools-vite"
import { tanstackStart } from "@tanstack/react-start/plugin/vite"
import viteReact from "@vitejs/plugin-react"
import viteTsConfigPaths from "vite-tsconfig-paths"
import tailwindcss from "@tailwindcss/vite"
import { nitro } from "nitro/vite"
import { nodeGuardVerdict, pinnedNodeMajor } from "./scripts/lib/node-guard"

/**
 * Refuse to start the DEV SERVER on a Node that silently breaks it (EXP-632).
 *
 * The rules and the message live in `scripts/lib/node-guard.ts` (unit-tested);
 * this plugin only supplies the real values. It runs from `config()` on
 * `command === "serve"`, so it fires for every dev serve however it was
 * invoked (`bun dev`, `vite --host`, an editor task) and never for `vite
 * build` — the production output is served by Bun, which is unaffected.
 */
function nodeVersionGuard(): PluginOption {
  return {
    name: `exp:node-version-guard`,
    // Ordered first in `plugins` so the throw beats every other plugin's
    // setup work.
    config(_config, { command }) {
      if (command !== `serve`) return
      if (process.env.VITEST) return

      let toolVersions = ``
      try {
        // Repo root, two levels up from apps/web — this resolves through a
        // git WORKTREE too, since it is relative to this file, not to cwd.
        toolVersions = readFileSync(
          new URL(`../../.tool-versions`, import.meta.url),
          `utf8`
        )
      } catch {
        /* no .tool-versions is not fatal */
      }

      const verdict = nodeGuardVerdict({
        current: process.versions.node,
        pinned: pinnedNodeMajor(toolVersions),
        isBun: Boolean(process.versions.bun),
      })
      if (verdict.kind === `fail`) throw new Error(verdict.message)
      if (verdict.message) console.warn(verdict.message)
    },
  }
}

const plugins: PluginOption[] = [
  nodeVersionGuard(),
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
