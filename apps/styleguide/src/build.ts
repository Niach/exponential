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

import { compileUiCss } from "@exp/ui/island"

import { COMPONENTS } from "./components.tsx"
import { renderHtml } from "./render.ts"
import { manualPairs, missingPairs, readGallery, storeDir } from "./store.ts"

const distDir = path.resolve(import.meta.dir, `..`, `dist`)

async function build(): Promise<void> {
  const source = storeDir()
  const data = readGallery(source)

  // `base` is THIS directory so the Tailwind scanner sees the island fixtures
  // in `components.tsx`; the package's own `@source` pulls in every component
  // it renders. One compile per build, ~1s.
  const uiCss = await compileUiCss({ base: import.meta.dir })

  rmSync(distDir, { recursive: true, force: true })
  mkdirSync(distDir, { recursive: true })
  writeFileSync(
    path.join(distDir, `index.html`),
    renderHtml(data, COMPONENTS, uiCss),
    `utf8`
  )

  if (existsSync(source)) {
    cpSync(source, path.join(distDir, `shots`), { recursive: true })
  }

  const total = data.counts.ok + data.counts.missing + data.counts.manual
  console.log(`styleguide → ${path.join(distDir, `index.html`)}`)
  console.log(`  ui css ${Math.round(uiCss.length / 1024)} KB · ${COMPONENTS.filter((spec) => spec.island !== undefined).length} islands`)
  console.log(
    `  ${data.views.length} views · ${data.counts.ok}/${total} captured · ${data.counts.manual} awaiting manual capture · ${data.counts.na} n/a · store ${existsSync(source) ? source : `${source} (absent)`}`
  )

  if (!process.argv.includes(`--check`)) return

  const missing = missingPairs(data)
  const manual = manualPairs(data)
  for (const pair of missing) console.log(`  missing  ${pair}`)
  for (const pair of manual) console.log(`  awaiting manual capture  ${pair}`)
  for (const stray of data.undeclared) console.log(`  undeclared  ${stray}`)
  // The exit code deliberately ignores `manual`: those pairs are waiting on a
  // person with a live session on screen, and a gate that goes red for them is
  // a gate everyone learns to ignore. They are printed, not enforced.
  if (missing.length > 0 || data.undeclared.length > 0) {
    console.error(
      `check failed: ${missing.length} missing, ${data.undeclared.length} undeclared`
    )
    process.exit(1)
  }
  console.log(`  check ok${manual.length > 0 ? ` (${manual.length} awaiting manual capture)` : ``}`)
}

await build()
