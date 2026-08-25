/**
 * Builds the static gallery into `dist/`: one HTML file plus a copy of the
 * store, so the output is a plain directory any static host (Coolify) can serve
 * and `open dist/index.html` works with no server at all.
 *
 * `--check` additionally reports every declared-but-uncaptured pair and every
 * undeclared file, exiting 1 when it finds any. A PLAIN build always succeeds —
 * a half-captured store is a normal state, not a build failure.
 */

import { cpSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs"
import path from "node:path"

import { renderHtml } from "./render.ts"
import { missingPairs, readGallery, storeDir } from "./store.ts"

const distDir = path.resolve(import.meta.dir, `..`, `dist`)

function build(): void {
  const source = storeDir()
  const data = readGallery(source)

  rmSync(distDir, { recursive: true, force: true })
  mkdirSync(distDir, { recursive: true })
  writeFileSync(path.join(distDir, `index.html`), renderHtml(data), `utf8`)

  if (existsSync(source)) {
    cpSync(source, path.join(distDir, `shots`), { recursive: true })
  }

  const total = data.counts.ok + data.counts.missing
  console.log(`styleguide → ${path.join(distDir, `index.html`)}`)
  console.log(
    `  ${data.views.length} views · ${data.counts.ok}/${total} captured · ${data.counts.na} n/a · store ${existsSync(source) ? source : `${source} (absent)`}`
  )

  if (!process.argv.includes(`--check`)) return

  const missing = missingPairs(data)
  for (const pair of missing) console.log(`  missing  ${pair}`)
  for (const stray of data.undeclared) console.log(`  undeclared  ${stray}`)
  if (missing.length > 0 || data.undeclared.length > 0) {
    console.error(
      `check failed: ${missing.length} missing, ${data.undeclared.length} undeclared`
    )
    process.exit(1)
  }
  console.log(`  check ok`)
}

build()
