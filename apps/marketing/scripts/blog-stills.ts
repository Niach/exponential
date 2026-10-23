/* Photographs the blog's live Exponential UI surfaces (components/blog/
   LiveSurface) into public/blog/<id>.webp, and, with --pages <dir>, the
   blog pages themselves for a Guide. Runs under NODE against a built dist
   (the prerendered pages + the renderer chunk):

     bun run build && node scripts/blog-stills.ts [--pages /tmp/blog-pages] [--skip-stills]

   Each surface's still stands in for the live render in the prerender, on
   the index card and in feed readers, so it is captured FROM that render at
   the article column's width (2x). cwebp (libwebp) encodes the files. */
import { execFileSync, spawn } from "node:child_process"
import { existsSync, mkdirSync, readdirSync, writeFileSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { chromium } from "playwright-core"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), `..`)
const OUT = resolve(ROOT, `public/blog`)
const PORT = 4179
const BASE = `http://127.0.0.1:${PORT}`
const PAGES_DIR = process.argv.includes(`--pages`) ? process.argv[process.argv.indexOf(`--pages`) + 1] : undefined
/* --skip-stills: only the pages (the stills are already in the build). */
const SKIP_STILLS = process.argv.includes(`--skip-stills`)

/* A browser Playwright knows (its registry) or the newest cached Chromium. */
function chromiumPath(): string | undefined {
  const cache = resolve(process.env.HOME ?? ``, `.cache/ms-playwright`)
  if (!existsSync(cache)) return undefined
  const builds = readdirSync(cache).filter((d) => /^chromium-\d+$/.test(d)).sort()
  for (const build of builds.reverse()) {
    for (const bin of [`chrome-linux64/chrome`, `chrome-linux/chrome`]) {
      const path = resolve(cache, build, bin)
      if (existsSync(path)) return path
    }
  }
  return undefined
}

function toWebp(png: Buffer, file: string): void {
  const tmp = `${file}.png`
  writeFileSync(tmp, png)
  execFileSync(`cwebp`, [`-quiet`, `-q`, `88`, tmp, `-o`, file])
  execFileSync(`rm`, [tmp])
}

async function main() {
  const dist = resolve(ROOT, `dist`)
  if (!existsSync(resolve(dist, `blog/index.html`))) throw new Error(`dist/blog missing: run bun run build first`)
  const server = spawn(`node`, [resolve(ROOT, `../../node_modules/vite/bin/vite.js`), `preview`, `--port`, String(PORT), `--strictPort`], { cwd: ROOT, stdio: `ignore` })
  const browser = await chromium.launch({ executablePath: chromiumPath() })
  try {
    for (let i = 0; i < 50; i++) {
      try {
        const res = await fetch(`${BASE}/blog/`)
        if (res.ok) break
      } catch {
        /* not up yet */
      }
      await new Promise((r) => setTimeout(r, 200))
    }
    mkdirSync(OUT, { recursive: true })

    const posts = readdirSync(resolve(dist, `blog`), { withFileTypes: true })
      .filter((d) => d.isDirectory())
      .map((d) => d.name)

    /* Stills: one page per post at the article width, 2x. */
    const page = await browser.newPage({ viewport: { width: 1200, height: 900 }, deviceScaleFactor: 2 })
    for (const slug of SKIP_STILLS ? [] : posts) {
      await page.goto(`${BASE}/blog/${slug}/`, { waitUntil: `networkidle` })
      /* The still is the frame alone: no concept stamp (the page draws it
         over the still too) and no sticky top bar over a scrolled frame. */
      await page.addStyleTag({ content: `.topbar, .blog-concept { display: none !important }` })
      const surfaces = page.locator(`figure.blog-surface`)
      const count = await surfaces.count()
      if (count === 0) continue
      for (let i = 0; i < count; i++) await surfaces.nth(i).scrollIntoViewIfNeeded()
      await page.waitForFunction(() => [...document.querySelectorAll(`figure.blog-surface`)].every((el) => el.getAttribute(`data-live`) === `ready`), null, { timeout: 30000 })
      await page.evaluate(() => document.fonts.ready)
      await page.waitForTimeout(800)
      for (let i = 0; i < count; i++) {
        const figure = surfaces.nth(i)
        const id = await figure.getAttribute(`data-surface`)
        const frame = figure.locator(`.blog-surface-frame`)
        const box = await frame.boundingBox()
        const png = await frame.screenshot({ type: `png` })
        const file = resolve(OUT, `${id}.webp`)
        toWebp(png, file)
        console.log(`still ${id}: ${Math.round((box?.width ?? 0) * 2)}×${Math.round((box?.height ?? 0) * 2)} → public/blog/${id}.webp`)
      }
    }
    await page.close()

    /* Guide pictures: the index, every post and the home strip, web + phone. */
    if (PAGES_DIR) {
      mkdirSync(PAGES_DIR, { recursive: true })
      const shots: [string, string, (p: import("playwright-core").Page) => Promise<Buffer>][] = [
        [`blog-index`, `/blog/`, (p) => p.screenshot({ type: `png`, fullPage: true })],
        ...posts.map((slug): [string, string, (p: import("playwright-core").Page) => Promise<Buffer>] => [`blog-${slug}`, `/blog/${slug}/`, (p) => p.screenshot({ type: `png`, fullPage: true })]),
        [`home-blog-strip`, `/#blog`, (p) => p.locator(`section#blog`).screenshot({ type: `png` })],
      ]
      for (const [form, viewport] of [[`web`, { width: 1440, height: 900 }], [`web-mobile`, { width: 390, height: 844 }]] as const) {
        const p = await browser.newPage({ viewport, deviceScaleFactor: 1 })
        for (const [name, path, shoot] of shots) {
          await p.goto(`${BASE}${path}`, { waitUntil: `networkidle` })
          await p.evaluate(() => document.fonts.ready)
          /* Lazy images load only as they near the viewport: walk the page
             once, wait for every image, then shoot from the top. */
          await p.evaluate(async () => {
            const step = window.innerHeight
            for (let y = 0; y < document.body.scrollHeight; y += step) {
              window.scrollTo(0, y)
              await new Promise((r) => setTimeout(r, 120))
            }
            window.scrollTo(0, 0)
          })
          await p.waitForFunction(() => [...document.images].every((img) => img.complete), null, { timeout: 30000 })
          if (path.includes(`#blog`)) await p.locator(`section#blog`).scrollIntoViewIfNeeded()
          await p.waitForTimeout(1200)
          toWebp(await shoot(p), resolve(PAGES_DIR, `${name}.${form}.webp`))
          console.log(`page ${name} (${form})`)
        }
        await p.close()
      }
    }
  } finally {
    await browser.close()
    server.kill()
  }
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
