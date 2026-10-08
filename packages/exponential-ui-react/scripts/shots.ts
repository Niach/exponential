#!/usr/bin/env bun
// VAPP-87: the kitchen sink photographed through the harness at the store's
// web frames (1440×960 @2x and 390×844 @3x → the 1800 px long edge every
// stored shot has), encoded with the shots pipeline's one encoder and
// written to shots/exponential-ui-kitchen-sink/{web,web-mobile}.webp. The
// app route `/exponential-ui-kitchen-sink` is what `bun run shots` captures
// once the dev stack runs; this is the stack-free door.
//
//   bun run --filter @exponential-at/ui-react shots [--theme exponential] [--mode dark] [--out <dir>]

import { chromium } from "playwright"
import { spawn } from "node:child_process"
import { mkdirSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { encodeShot } from "../../shots/src/encode.ts"

const pkgRoot = join(import.meta.dirname, `..`)
const repoRoot = join(pkgRoot, `..`, `..`)
const argv = process.argv.slice(2)
const arg = (name: string, fallback: string) => {
  const i = argv.indexOf(`--${name}`)
  return i >= 0 ? argv[i + 1] : fallback
}
const theme = arg(`theme`, `exponential`)
const mode = arg(`mode`, `dark`)
const out = arg(`out`, join(repoRoot, `shots`, `exponential-ui-kitchen-sink`))

const port = 4600 + Math.floor(Math.random() * 300)
const proc = spawn(`bun`, [join(pkgRoot, `harness/serve.ts`)], { env: { ...process.env, PORT: String(port) }, stdio: [`ignore`, `inherit`, `inherit`] })
const base = `http://127.0.0.1:${port}`
for (let i = 0; i < 100; i++) {
  try {
    if ((await fetch(`${base}/dist/main.js`)).ok) break
  } catch {
    /* not up yet */
  }
  await new Promise((r) => setTimeout(r, 150))
}
const browser = await chromium.launch()
mkdirSync(out, { recursive: true })
try {
  for (const [platform, viewport, scale] of [
    [`web`, { width: 1440, height: 960 }, 2],
    [`web-mobile`, { width: 390, height: 844 }, 3],
  ] as const) {
    const context = await browser.newContext({ viewport, deviceScaleFactor: scale, isMobile: platform === `web-mobile` })
    const page = await context.newPage()
    await page.goto(`${base}/?view=kitchen-sink&theme=${theme}&mode=${mode}`)
    await page.waitForSelector(`[data-xui-id="root"]`)
    await page.waitForTimeout(600)
    const png = await page.screenshot({ type: `png`, fullPage: false })
    const encoded = await encodeShot(png)
    const file = join(out, `${platform}.webp`)
    writeFileSync(file, encoded.buf)
    console.log(`${file}  ${encoded.width}×${encoded.height}  ${encoded.buf.length} bytes`)
    await context.close()
  }
} finally {
  await browser.close()
  proc.kill()
}
