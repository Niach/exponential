#!/usr/bin/env bun
// FEED-131: pictures of the PRIMITIVE sets for a Guide or a review: the
// harness `?view=primitives` pages of both renderers (React = the
// reference, Solid = the candidate), each specimen clipped to its wrapper,
// composed into galleries with sharp. Wears the conformance font set like
// the parity gate, so the pictures show the fonts the gate compares. Writes
// to /tmp/feed131/primitive-shots/ by default, never into shots/.
//
//   bun run --filter @exponential-at/ui-solid shots:primitives [--out <dir>]

import { chromium, type Browser, type Page } from "playwright"
import { spawn, type ChildProcess } from "node:child_process"
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import sharp from "sharp"
import { fontFaceCss, fontFiles, MANIFEST_PATH, type ConformanceManifest, type FontManifest } from "../../exponential-ui/conformance/dump"

const pkgRoot = join(import.meta.dirname, `..`)
const repoRoot = join(pkgRoot, `..`, `..`)
const argv = process.argv.slice(2)
const out = (() => {
  const i = argv.indexOf(`--out`)
  return i >= 0 ? argv[i + 1]! : `/tmp/feed131/primitive-shots`
})()

const manifest = JSON.parse(readFileSync(join(repoRoot, MANIFEST_PATH), `utf8`)) as ConformanceManifest
const fonts = JSON.parse(readFileSync(join(repoRoot, manifest.fonts), `utf8`)) as FontManifest
const fontAllow = new Set(fontFiles(fonts))
const fontCss = fontFaceCss(fonts, (file) => `/__xui-fonts/${file}`)

async function serve(script: string): Promise<{ base: string; proc: ChildProcess }> {
  const port = 4900 + Math.floor(Math.random() * 300)
  const proc = spawn(`bun`, [script], { env: { ...process.env, PORT: String(port) }, stdio: [`ignore`, `ignore`, `inherit`] })
  const base = `http://127.0.0.1:${port}`
  for (let i = 0; i < 600; i++) {
    try {
      if ((await fetch(`${base}/dist/main.js`)).ok) return { base, proc }
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 150))
  }
  throw new Error(`${script} never came up on ${base}`)
}

type Side = `react` | `solid`
interface Shot {
  side: Side
  id: string
  theme?: string
  mode?: `light` | `dark`
  viewport?: { width: number; height: number }
  /** Hover the first `[data-xui-target]` before the shot. */
  hover?: boolean
  /** Clip to the specimen wrapper's height (default) or keep the viewport. */
  full?: boolean
}

async function shot(browser: Browser, bases: Record<Side, string>, s: Shot): Promise<Buffer> {
  const viewport = s.viewport ?? { width: 900, height: 600 }
  const context = await browser.newContext({ viewport, deviceScaleFactor: 2, locale: `en-US`, timezoneId: `UTC`, reducedMotion: `reduce` })
  await context.route(`**/__xui-fonts/**`, (route) => {
    const rel = decodeURIComponent(new URL(route.request().url()).pathname.slice(`/__xui-fonts/`.length))
    return fontAllow.has(rel) ? route.fulfill({ status: 200, contentType: `font/ttf`, body: readFileSync(join(repoRoot, rel)) }) : route.abort()
  })
  await context.addInitScript({ content: `(() => { const add = () => { if (!document.documentElement) return false; const s = document.createElement('style'); s.textContent = ${JSON.stringify(fontCss)}; document.documentElement.appendChild(s); return true; }; if (!add()) { const mo = new MutationObserver(() => { if (add()) mo.disconnect(); }); mo.observe(document, { childList: true }); } })()` })
  const page: Page = await context.newPage()
  const q = new URLSearchParams({ view: `primitives`, id: s.id, theme: s.theme ?? `exponential`, mode: s.mode ?? `light`, width: String(viewport.width) })
  await page.goto(`${bases[s.side]}/?${q}`)
  await page.waitForSelector(`[data-xui-specimen]`, { state: `attached`, timeout: 30_000 })
  await page.evaluate(`document.fonts.ready`)
  await page.waitForTimeout(400)
  if (s.hover) {
    await page.hover(`[data-xui-target]`, { force: true })
    await page.waitForTimeout(400)
  }
  let clip: { x: number; y: number; width: number; height: number } | undefined
  if (!s.full) {
    const box = await page.locator(`[data-xui-specimen]`).boundingBox()
    if (box) clip = { x: 0, y: 0, width: viewport.width, height: Math.min(viewport.height, Math.ceil(box.y + box.height + 16)) }
  }
  const png = await page.screenshot({ type: `png`, clip })
  await context.close()
  return png
}

/** Tiles laid out in `columns`, each tile at its own height, rows packed. */
async function grid(tiles: Buffer[], columns: number, background: string, gap = 16): Promise<Buffer> {
  const metas = await Promise.all(tiles.map((t) => sharp(t).metadata()))
  const colWidth = Math.max(...metas.map((m) => m.width ?? 0))
  const rows: number[] = []
  for (let i = 0; i < tiles.length; i += columns) rows.push(Math.max(...metas.slice(i, i + columns).map((m) => m.height ?? 0)))
  const width = columns * colWidth + (columns + 1) * gap
  const height = rows.reduce((a, b) => a + b, 0) + (rows.length + 1) * gap
  const composite: sharp.OverlayOptions[] = []
  let y = gap
  tiles.forEach((t, i) => {
    const col = i % columns
    const row = Math.floor(i / columns)
    if (col === 0 && i > 0) y += rows[row - 1]! + gap
    composite.push({ input: t, left: gap + col * (colWidth + gap), top: y })
  })
  return sharp({ create: { width, height, channels: 4, background } }).composite(composite).png().toBuffer()
}

const bg = { light: `#ffffff`, dark: `#09090b` } as const

const [react, solid] = await Promise.all([serve(join(repoRoot, `packages/exponential-ui-react/harness/serve.ts`)), serve(join(pkgRoot, `harness/serve.ts`))])
const bases: Record<Side, string> = { react: react.base, solid: solid.base }
const browser = await chromium.launch({ executablePath: process.env.EXPONENTIAL_UI_CHROME || undefined, args: [`--font-render-hinting=none`, `--force-color-profile=srgb`] })
mkdirSync(out, { recursive: true })
try {
  const write = (name: string, buf: Buffer) => {
    writeFileSync(join(out, name), buf)
    console.log(`wrote ${join(out, name)}`)
  }
  // 1. Solid, exponential light: the plain set and the controls.
  const light = (id: string, hover = false) => shot(browser, bases, { side: `solid`, id, hover })
  write(`web-solid-primitives-light.png`, await grid(await Promise.all([light(`plain/button`), light(`plain/badge`), light(`plain/input-textarea`), light(`controls/checkbox`), light(`controls/switch`), light(`controls/tabs`), light(`plain/card`), light(`plain/table`), light(`controls/avatar`), light(`plain/progress`), light(`plain/choice-tiles`), light(`plain/breadcrumb`)]), 2, bg.light))
  // 2. Solid, orange dark: the overlays, the select, the command, the calendar.
  const dark = (id: string, hover = false) => shot(browser, bases, { side: `solid`, id, theme: `orange`, mode: `dark`, hover, full: true })
  write(`web-solid-overlays-orange-dark.png`, await grid(await Promise.all([dark(`overlays/dialog-open`), dark(`overlays/dropdown-menu-sub`), dark(`controls/select-open`), dark(`overlays/toast-stack`, true), dark(`calendar/single`), dark(`controls/command`)]), 3, bg.dark))
  // 3. React vs Solid side by side, orange dark.
  const pair = async (id: string) => [await shot(browser, bases, { side: `react`, id, theme: `orange`, mode: `dark`, full: true }), await shot(browser, bases, { side: `solid`, id, theme: `orange`, mode: `dark`, full: true })]
  write(`web-react-vs-solid-primitives.png`, await grid([...(await pair(`overlays/dropdown-menu-sub`)), ...(await pair(`calendar/dropdown`)), ...(await pair(`plain/table`))], 2, bg.dark))
  // 4. The phone: the dialog's sheet arm and the bottom sheet.
  const phone = (id: string) => shot(browser, bases, { side: `solid`, id, theme: `orange`, mode: `dark`, viewport: { width: 390, height: 800 }, full: true })
  write(`web-mobile-solid-sheets.png`, await grid([await phone(`overlays/dialog-sheet-phone`), await phone(`overlays/sheet-bottom`)], 2, bg.dark))
} finally {
  await browser.close()
  react.proc.kill()
  solid.proc.kill()
}
