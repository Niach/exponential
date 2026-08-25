/* App Store / Play Store screenshot compositor (EXP-566).

     bun run screenshots:store                     # PRODUCTION_SET → the upload dirs
     bun run screenshots:store --proposals         # all 3 style sets + a contact sheet
     bun run screenshots:store --set aurora        # override the shipping set
     bun run screenshots:store --platform ios      # ios | android | all
     bun run screenshots:store --slide steering    # one slide only (no dir clear)
     bun run screenshots:store --dry-run           # → apps/marketing/out/store-frames
     bun run screenshots:store --debug-crops       # raws + stroked pop-out rects
     bun run screenshots:store --check             # validate, write nothing shippable

   Replaces the satori-based `scripts/frame-store-screenshots.tsx` (EXP-580).
   Rendering is React SSR → static HTML → headless Chromium screenshot, because
   the look depends on real shadows, blurs and rotations, and satori's SVG path
   loses all three (resvg panics outright on the shadow filter at store sizes).

   Layout is authored at HALF the canvas in CSS px and photographed at
   deviceScaleFactor 2, so every output lands on its exact required pixel size —
   App Store Connect rejects off-size 6.9"/13" images.

   This step OWNS the upload dirs: it clears them and writes the full composed
   set. The untouched captures live in the `screenshots-raw/` trees (see
   raw-store.ts, which also migrates the older in-place layout). Nothing here is
   committed — store screenshots stay out of git (EXP-348). */

import { renderToStaticMarkup } from "react-dom/server"
import { chromium, type Browser, type BrowserContext, type Page } from "@playwright/test"
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs"
import { basename, dirname, join, resolve } from "node:path"

import {
  LAYOUT,
  MARKETING,
  pngDataUri,
  pngSize,
  syncRaws,
  withMarker,
  type Form,
  type Platform,
  type Raw,
} from "./raw-store"
import { debugRects, popRect, type Rect } from "./store-crops"
import {
  DEVICES,
  FEATURE_GRAPHIC,
  PRODUCTION_SET,
  SLIDES,
  STYLE_SET_IDS,
  playRatioOk,
  slideFileName,
  slidesFor,
  type DeviceSpec,
  type Slide,
  type StyleSetId,
} from "./store-slides"
import { CSS_SCALE, STYLE_SETS, type RenderInput } from "./store-styles"

const OUT = resolve(MARKETING, `out`)

/** Headlines below this many OUTPUT pixels are unreadable on a store card. */
const MIN_HEADLINE_PX = 40

/* ── Assets, inlined: the page must render with zero network. ─────────────── */

const FONT_CSS = (() => {
  const woff2 = readFileSync(resolve(MARKETING, `public/fonts/geist-latin.woff2`)).toString(`base64`)
  return `@font-face{font-family:"Geist";font-style:normal;font-weight:300 700;font-display:block;src:url(data:font/woff2;base64,${woff2}) format("woff2");}`
})()

const LOGO = (() => {
  const svg = readFileSync(resolve(MARKETING, `public/logo-light.svg`)).toString(`base64`)
  return `data:image/svg+xml;base64,${svg}`
})()

function htmlDoc(body: string, w: number, h: number): string {
  return [
    `<!doctype html><html><head><meta charset="utf-8">`,
    `<style>${FONT_CSS}`,
    `*{box-sizing:border-box;margin:0;padding:0}`,
    `html,body{width:${w}px;height:${h}px;overflow:hidden;background:#09090B}`,
    `</style></head><body>${body}</body></html>`,
  ].join(``)
}

/* ── CLI ─────────────────────────────────────────────────────────────────── */

type Args = {
  proposals: boolean
  set: StyleSetId
  platforms: Platform[]
  slide: string | null
  dryRun: boolean
  debugCrops: boolean
  check: boolean
}

function parseArgs(argv: string[]): Args {
  const flag = (name: string) => argv.includes(`--${name}`)
  const value = (name: string) => {
    const i = argv.indexOf(`--${name}`)
    return i >= 0 ? (argv[i + 1] ?? null) : null
  }
  const setArg = value(`set`)
  if (setArg && !STYLE_SET_IDS.includes(setArg as StyleSetId)) {
    throw new Error(`unknown --set ${setArg} (have ${STYLE_SET_IDS.join(`, `)})`)
  }
  const platformArg = value(`platform`)
  if (platformArg && ![`ios`, `android`, `all`].includes(platformArg)) {
    throw new Error(`unknown --platform ${platformArg}`)
  }
  const slide = value(`slide`)
  if (slide && !SLIDES.some((s) => s.id === slide)) {
    throw new Error(`unknown --slide ${slide} (have ${SLIDES.map((s) => s.id).join(`, `)})`)
  }
  return {
    proposals: flag(`proposals`),
    set: (setArg as StyleSetId | null) ?? (PRODUCTION_SET as StyleSetId),
    platforms: !platformArg || platformArg === `all` ? [`ios`, `android`] : [platformArg as Platform],
    slide,
    dryRun: flag(`dry-run`),
    debugCrops: flag(`debug-crops`),
    check: flag(`check`),
  }
}

/* ── Renderer ────────────────────────────────────────────────────────────── */

class Shooter {
  private browser!: Browser
  private contexts = new Map<number, BrowserContext>()
  private pages = new Map<number, Page>()

  async open() {
    this.browser = await chromium.launch()
  }

  async close() {
    await this.browser?.close()
  }

  /** One context + one page per device-scale factor, reused across slides. */
  private async page(scale: number): Promise<Page> {
    const cached = this.pages.get(scale)
    if (cached) return cached
    const context = await this.browser.newContext({ deviceScaleFactor: scale, colorScheme: `dark` })
    const page = await context.newPage()
    this.contexts.set(scale, context)
    this.pages.set(scale, page)
    return page
  }

  async shoot(html: string, cssW: number, cssH: number, scale: number): Promise<{ png: Buffer; problems: string[] }> {
    const page = await this.page(scale)
    await page.setViewportSize({ width: cssW, height: cssH })
    await page.setContent(html, { waitUntil: `load` })
    await page.evaluate(async () => {
      await document.fonts.load(`700 100px Geist`)
      await document.fonts.ready
      await Promise.all(Array.from(document.images).map((img) => img.decode().catch(() => undefined)))
    })
    const problems = await page.evaluate((minOut: number) => {
      const found: string[] = []
      const dpr = window.devicePixelRatio || 1
      for (const el of Array.from(document.querySelectorAll(`[data-probe="text"]`))) {
        const node = el as HTMLElement
        if (node.scrollHeight > node.clientHeight + 1 || node.scrollWidth > node.clientWidth + 1) {
          found.push(`clipped text: "${(node.textContent ?? ``).slice(0, 48).trim()}"`)
        }
      }
      for (const el of Array.from(document.querySelectorAll(`[data-headline]`))) {
        const px = parseFloat(getComputedStyle(el).fontSize) * dpr
        if (px < minOut) found.push(`headline "${(el.textContent ?? ``).trim()}" is only ${Math.round(px)} output px`)
      }
      return found
    }, MIN_HEADLINE_PX)
    const png = await page.screenshot({ type: `png`, fullPage: false })
    return { png: Buffer.from(png), problems }
  }
}

function buildInput(
  slide: Slide,
  form: Form,
  canvas: { width: number; height: number },
  raw: Raw | null
): RenderInput {
  const device = DEVICES[form]
  return {
    slide,
    form,
    canvas,
    shotUri: raw ? pngDataUri(raw.path) : null,
    rawWidth: raw?.width ?? device.rawWidth,
    rawHeight: raw?.height ?? device.rawHeight,
    pop: raw && !slide.noPop ? popRect(slide.shot, form, raw.path) : null,
    logo: LOGO,
  }
}

function canvasFor(slide: Slide, device: DeviceSpec): { width: number; height: number } {
  return slide.featureGraphic ? { ...FEATURE_GRAPHIC } : { width: device.width, height: device.height }
}

/* ── Raw index ───────────────────────────────────────────────────────────── */

type Index = { raws: Map<string, Raw>; problems: string[] }

/** Every file name this compositor writes into a platform's upload dir. */
function ownOutputs(platform: Platform): Set<string> {
  const names = new Set<string>()
  for (const form of FORMS_BY_PLATFORM[platform]) {
    const device = DEVICES[form]
    for (const slide of slidesFor(form)) {
      names.add(slide.featureGraphic ? `featureGraphic.png` : basename(productionPath(slide, device)))
    }
  }
  return names
}

function indexRaws(platforms: Platform[]): Index {
  const raws = new Map<string, Raw>()
  const problems: string[] = []
  for (const platform of platforms) {
    for (const raw of syncRaws(platform, ownOutputs(platform))) {
      const device = DEVICES[raw.form]
      if (device.strictRawSize && (raw.width !== device.width || raw.height !== device.height)) {
        problems.push(
          `${raw.file}: raw is ${raw.width}×${raw.height}, ${device.label} needs exactly ${device.width}×${device.height}`
        )
      }
      raws.set(`${raw.form}:${raw.shot}`, raw)
    }
  }
  return { raws, problems }
}

/** Hero has no shot of its own; it borrows one as decoration. */
function rawFor(slide: Slide, form: Form, raws: Map<string, Raw>): Raw | null {
  const key = slide.shot ?? slide.decorShot
  if (!key) return null
  return raws.get(`${form}:${key}`) ?? null
}

/* ── Output paths ────────────────────────────────────────────────────────── */

function productionPath(slide: Slide, device: DeviceSpec): string {
  const { upload } = LAYOUT[device.platform]
  if (slide.featureGraphic) return resolve(upload, `..`, `featureGraphic.png`)
  const name = slideFileName(slide)
  return join(upload, device.filePrefix ? `${device.filePrefix}-${name}` : name)
}

function clearUploads(platforms: Platform[]) {
  for (const platform of platforms) {
    const { upload } = LAYOUT[platform]
    if (!existsSync(upload)) continue
    for (const f of readdirSync(upload)) if (f.endsWith(`.png`)) rmSync(join(upload, f))
  }
}

/* ── Modes ───────────────────────────────────────────────────────────────── */

const FORMS_BY_PLATFORM: Record<Platform, Form[]> = {
  ios: [`ios-phone`, `ios-tablet`],
  android: [`android-phone`],
}

type Written = { form: Form; slide: Slide; path: string; width: number; height: number; problems: string[] }

async function renderSet(
  shooter: Shooter,
  setId: StyleSetId,
  args: Args,
  index: Index,
  scale: number,
  destFor: (slide: Slide, device: DeviceSpec) => string,
  stamp: boolean
): Promise<Written[]> {
  const written: Written[] = []
  const render = STYLE_SETS[setId]
  for (const platform of args.platforms) {
    for (const form of FORMS_BY_PLATFORM[platform]) {
      const device = DEVICES[form]
      for (const slide of slidesFor(form)) {
        if (args.slide && slide.id !== args.slide) continue
        const raw = rawFor(slide, form, index.raws)
        if ((slide.shot || slide.decorShot) && !raw) {
          index.problems.push(`${form}/${slide.id}: no raw capture for "${slide.shot ?? slide.decorShot}"`)
          // A slide whose whole point is the screenshot is worth nothing without
          // it — skip rather than shipping a half-empty card.
          if (slide.shot) continue
        }
        const canvas = canvasFor(slide, device)
        const cssW = canvas.width / CSS_SCALE
        const cssH = canvas.height / CSS_SCALE
        const body = renderToStaticMarkup(render(buildInput(slide, form, canvas, raw)) as never)
        const { png, problems } = await shooter.shoot(htmlDoc(body, cssW, cssH), cssW, cssH, scale)
        const out = stamp ? withMarker(png) : png
        const dest = destFor(slide, device)
        mkdirSync(dirname(dest), { recursive: true })
        writeFileSync(dest, out)
        const size = pngSize(out)
        written.push({ form, slide, path: dest, width: size.width, height: size.height, problems })
        console.log(`[${setId}] ${form}/${slide.id} → ${size.width}×${size.height}  ${dest.replace(`${MARKETING}/`, ``)}`)
      }
    }
  }
  return written
}

const PROPOSAL_DIR: Record<Form, string> = {
  "ios-phone": `ios-phone`,
  "ios-tablet": `ios-tablet`,
  "android-phone": `android`,
}

function contactSheet(root: string, sets: StyleSetId[], written: Map<StyleSetId, Written[]>) {
  const forms = Array.from(new Set(Array.from(written.values()).flat().map((w) => w.form)))
  const rel = (p: string) => p.slice(root.length + 1)
  const sections = forms
    .map((form, i) => {
      const rows = sets
        .map((set) => {
          const cells = (written.get(set) ?? [])
            .filter((w) => w.form === form)
            .map(
              (w) =>
                `<a class="cell" href="${rel(w.path)}" target="_blank"><img src="${rel(w.path)}" loading="lazy"><span>${w.slide.id}</span></a>`
            )
            .join(``)
          return `<div class="row"><h3>${set}</h3><div class="strip">${cells}</div></div>`
        })
        .join(``)
      return `<section data-form="${form}"${i === 0 ? `` : ` hidden`}>${rows}</section>`
    })
    .join(``)
  const tabs = forms
    .map((f, i) => `<button data-form="${f}"${i === 0 ? ` class="on"` : ``}>${f}</button>`)
    .join(``)
  const html = [
    `<!doctype html><html><head><meta charset="utf-8"><title>Store screenshot proposals</title><style>`,
    `body{background:#09090b;color:#fafafa;font:14px system-ui,sans-serif;margin:0;padding:32px}`,
    `h1{font-size:20px;margin:0 0 4px}p{color:#a1a1aa;margin:0 0 24px}`,
    `nav{display:flex;gap:8px;margin-bottom:24px}`,
    `nav button{background:#18181b;color:#a1a1aa;border:1px solid #27272a;border-radius:8px;padding:8px 14px;cursor:pointer;font:inherit}`,
    `nav button.on{background:#fafafa;color:#09090b;border-color:#fafafa}`,
    `.row{margin-bottom:32px}h3{margin:0 0 10px;font-size:13px;letter-spacing:.12em;text-transform:uppercase;color:#71717a}`,
    `.strip{display:flex;gap:12px;overflow-x:auto;padding-bottom:8px}`,
    `.cell{flex:0 0 auto;display:flex;flex-direction:column;gap:6px;text-decoration:none;color:#71717a;font-size:11px}`,
    `.cell img{width:150px;border-radius:8px;border:1px solid #27272a;display:block;background:#000}`,
    `</style></head><body><h1>Store screenshot proposals</h1><p>Three candidate style sets. Pick one and set <code>PRODUCTION_SET</code> in store-slides.ts.</p>`,
    `<nav>${tabs}</nav>${sections}`,
    `<script>document.querySelectorAll('nav button').forEach(function(b){b.onclick=function(){`,
    `document.querySelectorAll('nav button').forEach(function(x){x.classList.remove('on')});b.classList.add('on');`,
    `document.querySelectorAll('section').forEach(function(s){s.hidden=s.dataset.form!==b.dataset.form})}})</script>`,
    `</body></html>`,
  ].join(``)
  writeFileSync(join(root, `index.html`), html)
}

async function debugCrops(shooter: Shooter, args: Args, index: Index) {
  const root = join(OUT, `store-debug`)
  mkdirSync(root, { recursive: true })
  for (const [key, raw] of index.raws) {
    if (args.slide && raw.shot !== args.slide) continue
    const rects = debugRects(raw.shot, raw.form, raw.path)
    const uri = pngDataUri(raw.path)
    const boxes = rects
      .map(
        (r: { label: string; rect: Rect }, i: number) => `
        <div style="position:absolute;left:${r.rect.x * 100}%;top:${r.rect.y * 100}%;width:${r.rect.w * 100}%;height:${r.rect.h * 100}%;
          outline:6px solid ${i === 0 ? `#F43F5E` : `#5EEAD4`};">
          <span style="position:absolute;left:0;top:-42px;background:${i === 0 ? `#F43F5E` : `#5EEAD4`};color:#000;
            font:600 26px system-ui,sans-serif;padding:4px 10px;white-space:nowrap">${r.label} · x${r.rect.x} y${r.rect.y} w${r.rect.w} h${r.rect.h}</span>
        </div>`
      )
      .join(``)
    const body = `<div style="position:relative;width:${raw.width}px;height:${raw.height}px"><img src="${uri}" style="width:100%;height:100%;display:block">${boxes}</div>`
    const { png } = await shooter.shoot(htmlDoc(body, raw.width, raw.height), raw.width, raw.height, 1)
    const dest = join(root, `${key.replace(`:`, `_`)}.png`)
    writeFileSync(dest, png)
    console.log(`[debug] ${key} ${rects.length} rect(s) → ${dest.replace(`${MARKETING}/`, ``)}`)
  }
  if (index.raws.size === 0) console.warn(`no raws found — nothing to debug`)
}

/* ── main ────────────────────────────────────────────────────────────────── */

async function main() {
  const args = parseArgs(process.argv.slice(2))
  // syncRaws runs BEFORE anything clears an upload dir: under the legacy layout
  // the only copy of a fresh capture is sitting in there.
  const index = indexRaws(args.platforms)

  const shooter = new Shooter()
  await shooter.open()
  try {
    if (args.debugCrops) {
      await debugCrops(shooter, args, index)
      return
    }

    if (args.proposals) {
      const root = join(OUT, `store-proposals`)
      rmSync(root, { recursive: true, force: true })
      const written = new Map<StyleSetId, Written[]>()
      for (const set of STYLE_SET_IDS) {
        written.set(
          set,
          await renderSet(shooter, set, args, index, 1, (slide, device) =>
            join(root, set, PROPOSAL_DIR[device.form], slideFileName(slide))
          , false)
        )
      }
      contactSheet(root, [...STYLE_SET_IDS], written)
      console.log(`\ncontact sheet → ${join(root, `index.html`)}`)
      return
    }

    const checkOnly = args.check && !args.dryRun
    const dryRoot = join(OUT, checkOnly ? `store-check` : `store-frames`)
    const production = !args.dryRun && !checkOnly
    if (production && !args.slide) clearUploads(args.platforms)
    else if (production) console.warn(`--slide set: leaving the rest of the upload dir alone`)

    const written = await renderSet(
      shooter,
      args.set,
      args,
      index,
      CSS_SCALE,
      (slide, device) =>
        production
          ? productionPath(slide, device)
          : join(dryRoot, device.platform, `${device.form}-${slideFileName(slide)}`),
      true
    )

    /* ── validation ── */
    const failures = [...index.problems]
    for (const w of written) {
      const device = DEVICES[w.form]
      const want = canvasFor(w.slide, device)
      if (w.width !== want.width || w.height !== want.height) {
        failures.push(`${w.form}/${w.slide.id}: wrote ${w.width}×${w.height}, expected ${want.width}×${want.height}`)
      }
      // The side-ratio rule is a phone-SCREENSHOT rule; Play mandates exactly
      // 1024×500 for the feature graphic, which breaks it by design.
      if (device.platform === `android` && !w.slide.featureGraphic && !playRatioOk(w.width, w.height)) {
        failures.push(`${w.form}/${w.slide.id}: ${w.width}×${w.height} breaks Play's 2:1 max side ratio`)
      }
      for (const p of w.problems) failures.push(`${w.form}/${w.slide.id}: ${p}`)
    }

    console.log(
      `\nwrote ${written.length} image(s) with the "${args.set}" set` +
        (production ? `` : ` → ${dryRoot.replace(`${MARKETING}/`, ``)}`)
    )
    if (failures.length > 0) {
      console.error(`\n${failures.length} problem(s):`)
      for (const f of failures) console.error(`  - ${f}`)
      if (args.check) process.exit(1)
    } else if (args.check) {
      console.log(`check: clean`)
    }
  } finally {
    await shooter.close()
  }
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
