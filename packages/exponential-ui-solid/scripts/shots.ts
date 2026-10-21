#!/usr/bin/env bun
// FEED-128: the Solid twin of React's scripts/shots.ts: the harness kitchen
// sink at the store's web frames (1440×960 @2x and 390×844 @3x), encoded with
// the shots pipeline's one encoder. It writes to /tmp/feed128/solid-shots/
// by default, NEVER into shots/: the view catalog's `web` slot is React's.
//
//   bun run --filter @exponential-at/ui-solid shots [--theme orange] [--mode dark] [--out <dir>]

import { chromium } from "playwright"
import { spawn } from "node:child_process"
import { mkdirSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { encodeShot } from "../../shots/src/encode.ts"

const pkgRoot = join(import.meta.dirname, `..`)
const argv = process.argv.slice(2)
const arg = (name: string, fallback: string) => {
  const i = argv.indexOf(`--${name}`)
  return i >= 0 ? argv[i + 1] : fallback
}
const theme = arg(`theme`, `orange`)
const mode = arg(`mode`, `dark`)
const out = arg(`out`, `/tmp/feed128/solid-shots`)

const port = 4900 + Math.floor(Math.random() * 300)
const proc = spawn(`bun`, [join(pkgRoot, `harness/serve.ts`)], { env: { ...process.env, PORT: String(port) }, stdio: [`ignore`, `inherit`, `inherit`] })
const base = `http://127.0.0.1:${port}`
// The first /dist/main.js request runs the Vite build: give it a minute.
for (let i = 0; i < 400; i++) {
  try {
    if ((await fetch(`${base}/dist/main.js`)).ok) break
  } catch {
    /* not up yet */
  }
  await new Promise((r) => setTimeout(r, 150))
}
const browser = await chromium.launch({ executablePath: process.env.EXPONENTIAL_UI_CHROME || undefined })
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
    const encoded = await encodeShot(await page.screenshot({ type: `png`, fullPage: false }))
    const file = join(out, `${platform}.webp`)
    writeFileSync(file, encoded.buf)
    console.log(`${file}  ${encoded.width}×${encoded.height}  ${encoded.buf.length} bytes`)
    await context.close()
  }
} finally {
  await browser.close()
  proc.kill()
}
