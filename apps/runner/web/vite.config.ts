// VAPP-269: the web runner's static export. `dist/` is the whole runner:
// the shell, `app/` (runner.json + package.app.json, what E7 writes), the
// web manifest + icons (E7 regenerates them) and `sw.js`.

import { createHash } from "node:crypto"
import { readdirSync, readFileSync, statSync } from "node:fs"
import { join, relative } from "node:path"
import react from "@vitejs/plugin-react"
import { defineConfig, type Plugin } from "vite"

import { serviceWorkerSource } from "./src/sw"

const walk = (dir: string): string[] => readdirSync(dir).flatMap((n) => (statSync(join(dir, n)).isDirectory() ? walk(join(dir, n)) : [join(dir, n)]))

/** Writes `sw.js` precaching every emitted file and every public file. */
const serviceWorker = (): Plugin => ({
  name: `exp-runner-sw`,
  apply: `build`,
  generateBundle(_, bundle) {
    const publicDir = join(import.meta.dirname, `public`)
    const hash = createHash(`sha256`)
    const files = [`./`]
    for (const [file, chunk] of Object.entries(bundle).sort(([a], [b]) => (a < b ? -1 : 1))) {
      files.push(`./${file}`)
      hash.update(file).update(chunk.type === `chunk` ? chunk.code : chunk.source)
    }
    for (const path of walk(publicDir).sort()) {
      const file = relative(publicDir, path).split(`\\`).join(`/`)
      files.push(`./${file}`)
      hash.update(file).update(readFileSync(path))
    }
    this.emitFile({ type: `asset`, fileName: `sw.js`, source: serviceWorkerSource(`exp-runner-${hash.digest(`hex`).slice(0, 16)}`, files) })
  },
})

export default defineConfig({
  base: `./`,
  plugins: [react(), serviceWorker()],
  // The contract packages hash with Node's sync createHash (src/node-crypto.ts).
  resolve: { alias: { "node:crypto": join(import.meta.dirname, `src/node-crypto.ts`) } },
  worker: { format: `es` },
  // sqlite-wasm finds its .wasm beside its module: keep it out of the pre-bundle.
  optimizeDeps: { exclude: [`@sqlite.org/sqlite-wasm`] },
  build: { target: `es2022`, chunkSizeWarningLimit: 4096 },
})
