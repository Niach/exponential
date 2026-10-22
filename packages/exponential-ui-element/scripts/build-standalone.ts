#!/usr/bin/env bun
// FEED-130: the SELF-CONTAINED bundle — `src/standalone.ts` (registers
// `<exponential-surface>`) with EVERY dependency inlined (solid-js,
// @kobalte/core, the lucide-solid fallbacks, @exponential-at/ui,
// @exponential-at/ui-solid) into ONE minified ES module at
// `<dir>/dist/standalone.js` (+ its sourcemap): the `<script type="module">`
// build for a page with no bundler. Vite + `vite-plugin-solid` (the Solid
// painter ships TSX sources inside the monorepo). release/build-npm.ts calls
// `buildStandalone`; `bun run build:standalone` builds this package.

import { readFileSync } from "node:fs"
import { join, resolve } from "node:path"
import { gzipSync } from "node:zlib"

/** Builds `<dir>/src/standalone.ts` into `<dir>/dist/standalone.js`; returns
 *  that path. */
export async function buildStandalone(dir: string): Promise<string> {
  const { build } = await import(`vite`)
  const { default: solid } = await import(`vite-plugin-solid`)
  // Vite's `build()` sets `process.env.NODE_ENV` to "production" when it is
  // unset; a later child `npm install` in the same process would then skip
  // devDependencies: restore it (as build-npm.ts's buildSolid does).
  const nodeEnv = process.env.NODE_ENV
  try {
    await build({
      configFile: false,
      root: dir,
      logLevel: `warn`,
      plugins: [solid()],
      resolve: { conditions: [`browser`] },
      define: { "process.env.NODE_ENV": JSON.stringify(`production`) },
      build: {
        outDir: join(dir, `dist`),
        emptyOutDir: false,
        minify: true,
        target: `es2022`,
        sourcemap: true,
        modulePreload: false,
        copyPublicDir: false,
        // One file by design: no chunk-size advice.
        chunkSizeWarningLimit: 4096,
        rollupOptions: {
          input: join(dir, `src`, `standalone.ts`),
          // Nothing external: the file is the whole runtime.
          external: [],
          preserveEntrySignatures: `exports-only`,
          output: { format: `es`, entryFileNames: `standalone.js`, inlineDynamicImports: true },
        },
      },
    })
  } finally {
    if (nodeEnv === undefined) delete process.env.NODE_ENV
    else process.env.NODE_ENV = nodeEnv
  }
  const out = join(dir, `dist`, `standalone.js`)
  const bytes = readFileSync(out)
  console.log(`built ${out}: ${(bytes.length / 1024).toFixed(1)} kB, ${(gzipSync(bytes, { level: 9 }).length / 1024).toFixed(1)} kB gzip`)
  return out
}

if (import.meta.main) await buildStandalone(resolve(import.meta.dir, `..`))
