#!/usr/bin/env bun
// FEED-130: the SELF-CONTAINED bundle — `src/standalone.ts` (registers
// `<exponential-surface>`) with EVERY dependency inlined (solid-js,
// @kobalte/core, the lucide-solid fallbacks, @exponential_app/ui,
// @exponential_app/ui-solid) into ONE minified ES module at
// `<dir>/dist/standalone.js` (+ its sourcemap): the `<script type="module">`
// build for a page with no bundler. Vite + `vite-plugin-solid` (the Solid
// painter ships TSX sources inside the monorepo). release/build-npm.ts calls
// `buildStandalone`; `bun run build:standalone` builds this package.

import { readFileSync } from "node:fs"
import { join, resolve } from "node:path"
import { gzipSync } from "node:zlib"

export interface StandaloneBundle {
  /** The built file (`<outDir>/standalone.js`). */
  file: string
  /** Module id → its bytes in the file (the size gate's split check, VAPP-106). */
  inputs: Record<string, number>
}

/** Builds `<dir>/src/standalone.ts` into `<dir>/dist/standalone.js` (+ its
 *  sourcemap); returns that path. */
export async function buildStandalone(dir: string): Promise<string> {
  const { file } = await bundleStandalone(dir, { outDir: join(dir, `dist`), sourcemap: true })
  const bytes = readFileSync(file)
  console.log(`built ${file}: ${(bytes.length / 1024).toFixed(1)} kB, ${(gzipSync(bytes, { level: 9 }).length / 1024).toFixed(1)} kB gzip`)
  return file
}

/** The standalone bundle into `outDir` (VAPP-106: the size gate builds it
 *  into a temp dir and reads what went in). */
export async function bundleStandalone(dir: string, options: { outDir: string; sourcemap?: boolean }): Promise<StandaloneBundle> {
  const { build } = await import(`vite`)
  const { default: solid } = await import(`vite-plugin-solid`)
  // Vite's `build()` sets `process.env.NODE_ENV` to "production" when it is
  // unset; a later child `npm install` in the same process would then skip
  // devDependencies: restore it (as build-npm.ts's buildSolid does).
  const nodeEnv = process.env.NODE_ENV
  let inputs: Record<string, number> = {}
  try {
    const result = await build({
      configFile: false,
      root: dir,
      logLevel: `warn`,
      plugins: [solid()],
      resolve: { conditions: [`browser`] },
      define: { "process.env.NODE_ENV": JSON.stringify(`production`) },
      build: {
        outDir: options.outDir,
        emptyOutDir: false,
        minify: true,
        target: `es2022`,
        sourcemap: options.sourcemap ?? false,
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
    const chunks = (Array.isArray(result) ? result : [result]).flatMap((r) => (`output` in r ? r.output : [])).filter((o) => o.type === `chunk`)
    inputs = Object.fromEntries(chunks.flatMap((c) => Object.entries(c.modules).map(([k, v]) => [k, v.renderedLength])))
  } finally {
    if (nodeEnv === undefined) delete process.env.NODE_ENV
    else process.env.NODE_ENV = nodeEnv
  }
  return { file: join(options.outDir, `standalone.js`), inputs }
}

if (import.meta.main) await buildStandalone(resolve(import.meta.dir, `..`))
