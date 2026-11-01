// EXP-1602: renders the lottie-web references of the eui-motion corpus.
//
//   bun run conformance/motion/render-refs.ts [name-filter]
//
// Playwright headless Chromium (deviceScaleFactor 1) + lottie-web (npm,
// `lottie_svg.min.js`: the SVG renderer), transparent background. Each comp
// is fitted into at most 400×400 (keeps aspect; the longer side = 400) and
// three frames are rendered: ip, ip + 0.5·(op − ip), ip + 0.9·(op − ip),
// rounded DOWN to whole composition frames. Output: refs/<name>@<frame>.png
// (losslessly recompressed) and refs/index.json. The Rust side
// (apps/desktop/crates/eui-motion/tests/corpus.rs) paints the same frames at
// the same size and compares with SSIM.

import { readdirSync, readFileSync, mkdirSync, rmSync, writeFileSync } from "node:fs"
import { join, normalize } from "node:path"
import sharp from "sharp"
import { launchChromium } from "../browser-page"

const here = import.meta.dir
const corpus = join(here, `corpus`)
const refs = join(here, `refs`)
const filter = process.argv[2]
const MAX = 400
const FRACTIONS = [0, 0.5, 0.9]

const lottieDir = join(here, `../../node_modules/lottie-web`)
const lottiePath = (() => {
  for (const base of [lottieDir, join(here, `../../../../node_modules/lottie-web`)]) {
    try {
      readFileSync(join(base, `package.json`))
      return base
    } catch {}
  }
  throw new Error(`lottie-web not installed (bun install)`)
})()
const lottieVersion = JSON.parse(readFileSync(join(lottiePath, `package.json`), `utf8`)).version as string
const lottieJs = readFileSync(join(lottiePath, `build/player/lottie_svg.min.js`), `utf8`)

/** The fitted size and the frames of a comp. */
export function plan(anim: { w: number; h: number; ip: number; op: number }): { width: number; height: number; frames: number[] } {
  const s = MAX / Math.max(anim.w, anim.h)
  const width = Math.max(1, Math.round(anim.w * s))
  const height = Math.max(1, Math.round(anim.h * s))
  const frames = [...new Set(FRACTIONS.map((f) => Math.floor(anim.ip + f * (anim.op - anim.ip))))]
  return { width, height, frames }
}

const server = Bun.serve({
  port: 0,
  hostname: `127.0.0.1`,
  fetch(req) {
    const path = decodeURIComponent(new URL(req.url).pathname)
    if (path === `/`) return new Response(`<!doctype html><html><head><style>html,body{margin:0;padding:0;background:transparent}#c{position:absolute;left:0;top:0}</style></head><body><div id="c"></div><script>${lottieJs}</script></body></html>`, { headers: { "content-type": `text/html` } })
    const file = normalize(join(corpus, path.replace(/^\/corpus\//, ``)))
    if (!file.startsWith(corpus)) return new Response(`no`, { status: 403 })
    try {
      return new Response(Bun.file(file))
    } catch {
      return new Response(`missing`, { status: 404 })
    }
  },
})

const names = readdirSync(corpus)
  .filter((f) => f.endsWith(`.json`))
  .map((f) => f.slice(0, -5))
  .filter((n) => !filter || n.includes(filter))
  .sort()

if (!filter) rmSync(refs, { recursive: true, force: true })
mkdirSync(refs, { recursive: true })
const indexPath = join(refs, `index.json`)
let index: { $comment?: string; renderer?: string; files: Record<string, { width: number; height: number; frames: number[] }> } = { files: {} }
try {
  if (filter) index = JSON.parse(readFileSync(indexPath, `utf8`))
} catch {}

const browser = await launchChromium()
const context = await browser.newContext({ deviceScaleFactor: 1, viewport: { width: 800, height: 800 } })
for (const name of names) {
  const anim = JSON.parse(readFileSync(join(corpus, `${name}.json`), `utf8`))
  const { width, height, frames } = plan(anim)
  const page = await context.newPage()
  page.on(`pageerror`, (e) => console.warn(`${name}: page error ${e.message}`))
  await page.goto(`http://127.0.0.1:${server.port}/`)
  const ok = await page.evaluate(
    async ({ name, width, height }) => {
      const c = document.getElementById(`c`)!
      c.style.width = `${width}px`
      c.style.height = `${height}px`
      const lottie = (window as any).lottie
      const a = lottie.loadAnimation({ container: c, renderer: `svg`, loop: false, autoplay: false, path: `/corpus/${name}.json`, rendererSettings: { preserveAspectRatio: `xMidYMid meet`, progressiveLoad: false } })
      ;(window as any).anim = a
      return await new Promise<boolean>((resolve) => {
        a.addEventListener(`DOMLoaded`, () => resolve(true))
        a.addEventListener(`data_failed`, () => resolve(false))
        setTimeout(() => resolve(false), 15000)
      })
    },
    { name, width, height },
  )
  if (!ok) console.warn(`${name}: lottie-web did not load in 15 s; rendering anyway`)
  for (const frame of frames) {
    await page.evaluate(async (f) => {
      const a = (window as any).anim
      a.setSubframe(false)
      // goToAndStop counts frames from the comp's ip.
      a.goToAndStop(f - a.firstFrame, true)
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
    }, frame)
    const shot = await page.screenshot({ omitBackground: true, clip: { x: 0, y: 0, width, height } })
    const png = await sharp(shot).png({ compressionLevel: 9, adaptiveFiltering: true }).toBuffer()
    writeFileSync(join(refs, `${name}@${frame}.png`), png)
  }
  index.files[name] = { width, height, frames }
  await page.close()
  console.log(`${name}: ${width}×${height} frames ${frames.join(`, `)}`)
}
const version = browser.version()
await browser.close()
server.stop()

index.$comment = `AUTO-GENERATED by render-refs.ts: the lottie-web references of the eui-motion corpus. Per file: the fitted size (longer side ${MAX}) and the frames (ip, ip + 0.5·(op − ip), ip + 0.9·(op − ip), floored).`
index.renderer = `lottie-web ${lottieVersion} (SVG renderer), Chromium ${version} headless, deviceScaleFactor 1, transparent background`
index.files = Object.fromEntries(Object.entries(index.files).sort(([a], [b]) => a.localeCompare(b)))
writeFileSync(indexPath, `${JSON.stringify({ $comment: index.$comment, renderer: index.renderer, files: index.files }, null, 2)}\n`)
