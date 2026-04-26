// Inject Bun's `idleTimeout: 255` into the Nitro/srvx-generated server entry.
//
// Why this is needed:
// TanStack Start's Nitro preset uses srvx's `serve(...)` which spreads the
// caller's `bun: {...}` options directly into `Bun.serve()`. By default the
// generated entry passes no `bun` block, so Bun.serve() falls back to its
// 10-second idleTimeout. Electric long-poll requests routinely block for
// 20s+ waiting for changes; on a Traefik → Bun keep-alive connection that
// blows up as `502 Bad Gateway error=EOF` for every long-poll, breaking
// real-time sync entirely.
//
// We intentionally do not own the generated entry, so we patch it after
// `vite build`. The patch is idempotent and fails loudly if Nitro's output
// pattern changes — better a build break than a silent regression.

import { readFileSync, writeFileSync } from "node:fs"

const FILE = `.output/server/index.mjs`
const SENTINEL = `bun: { idleTimeout: 255 }`
const NEEDLE = `  fetch: nitroApp.fetch\n});`

const source = readFileSync(FILE, `utf8`)

if (source.includes(SENTINEL)) {
  console.log(`[patch-server] already patched, skipping`)
  process.exit(0)
}

if (!source.includes(NEEDLE)) {
  console.error(
    `[patch-server] could not find expected serve(...) call in ${FILE}.\n` +
      `Nitro output likely changed — update scripts/patch-server.mjs.`
  )
  process.exit(1)
}

const replacement = `  fetch: nitroApp.fetch,\n  ${SENTINEL}\n});`
writeFileSync(FILE, source.replace(NEEDLE, replacement))
console.log(`[patch-server] injected ${SENTINEL} into ${FILE}`)
