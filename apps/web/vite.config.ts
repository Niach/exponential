import { readFileSync } from "node:fs"
import { defineConfig, type PluginOption } from "vite"
import { devtools } from "@tanstack/devtools-vite"
import { tanstackStart } from "@tanstack/react-start/plugin/vite"
import viteReact from "@vitejs/plugin-react"
import viteTsConfigPaths from "vite-tsconfig-paths"
import tailwindcss from "@tailwindcss/vite"
import { nitro } from "nitro/vite"

/**
 * Refuse to start the dev server on a Node that silently breaks it.
 *
 * On Node 26, `fetch` over a UNIX SOCKET resolves with the right status and the
 * right body and ZERO response headers (the same server over TCP is fine — an
 * undici regression). Nitro's dev worker is reached over exactly such a socket,
 * so every TanStack Start route is served with all of its headers stripped:
 * no `content-type` (Start itself then throws `expected content-type header to
 * be set` in the browser), no `set-cookie` (login cannot work), and no
 * `electric-handle`/`electric-offset` — which ARE the shape cursor, so every
 * Electric client re-requests `offset=-1` forever and no collection ever syncs.
 * Nothing about it looks like an error: the JSON is correct, the app renders,
 * and it just never fills in. It cost a full screenshot-store refresh (EXP-566)
 * before anyone noticed.
 *
 * `.tool-versions` already pins the supported major; this is the check that
 * makes the pin bite on machines with no version manager installed. Dev server
 * only — `vite build` runs the same config, and the production output is served
 * by Bun, which is unaffected.
 */
function assertDevNodeVersion(): void {
  const isDevServer =
    process.argv.some((arg) => arg === `dev` || arg === `serve`) && !process.env.VITEST
  if (!isDevServer) return

  const current = Number(process.versions.node.split(`.`)[0])
  let pinned: number | undefined
  try {
    const match = readFileSync(
      new URL(`../../.tool-versions`, import.meta.url),
      `utf8`
    ).match(/^nodejs\s+(\d+)/m)
    pinned = match ? Number(match[1]) : undefined
  } catch {
    /* no .tool-versions is not fatal */
  }
  if (pinned !== undefined && current === pinned) return

  const pin = pinned === undefined ? `the version in .tool-versions` : `Node ${pinned}`
  if (current >= 26) {
    throw new Error(
      [
        `Node ${process.versions.node} cannot serve this app in dev.`,
        `  Node 26 drops EVERY response header on unix-socket fetches, which is how`,
        `  nitro's dev worker is reached — so Electric never syncs, login cannot set a`,
        `  cookie, and the app renders but stays empty, with no error anywhere.`,
        `  Use ${pin} (\`.tool-versions\`), e.g. with Homebrew:`,
        `    PATH="/opt/homebrew/opt/node@${pinned ?? 24}/bin:$PATH" bun dev`,
        `  Or serve the production build, which runs under Bun and is unaffected:`,
        `    bun run build && PORT=5173 bun --env-file=.env .output/server/index.mjs`,
      ].join(`\n`)
    )
  }
  console.warn(
    `[vite] Node ${process.versions.node} is not ${pin} (.tool-versions) — dev serving is unverified on it.`
  )
}

assertDevNodeVersion()

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
