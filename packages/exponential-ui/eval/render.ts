#!/usr/bin/env bun
// VAPP-108: renders every recorded surface on the web at each corpus width
// with BOTH web renderers (the React harness and the Solid harness, headless
// Chromium, the exponential theme, dark), writes the pictures to
// eval/out/<id>/<renderer>-<width>.png (gitignored) and MEASURES the painted
// boxes (__xuiFrames): the vertical gaps between the root's children, the
// left inset of the first text, the painted height, whether anything
// overflows the viewport sideways. The two renderers' measures must be equal
// and their pictures pixel-identical (the web-parity gate makes them so); a
// difference is a finding, recorded as `differences`. The measurements go
// into eval/report.json `render` (committed) so the numbers are on record
// without the pictures.
//
//   bun eval/render.ts [--only <id,id>] [--theme exponential] [--mode dark]
//
// The natives go through the shots pipeline's specimen lane once a corpus
// surface is a view (follow-up on VAPP-108); this is the stack-free door.

import pixelmatch from "pixelmatch"
import { chromium, type Browser } from "playwright"
import sharp from "sharp"
import { spawn } from "node:child_process"
import { mkdirSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { reduceSurface } from "../src/reducer"
import type { FlatComponent, UiNode } from "../src/types"
import corpus from "./corpus.json" with { type: "json" }
import { parseReply, surfaceOf, widthsOf, type Corpus } from "./lib"
import { REPORT_PATH, buildReport, readFixture, readReport } from "./score"

const argv = process.argv.slice(2)
const opt = (name: string, fallback: string) => {
  const i = argv.indexOf(`--${name}`)
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback
}
const only = new Set(opt(`only`, ``).split(`,`).filter(Boolean))
const theme = opt(`theme`, `exponential`)
const mode = opt(`mode`, `dark`)
const OUT = join(import.meta.dir, `out`)

/** The two web renderers, each with its harness server (the same page, the
 *  same `?view=tree` query, the same `__xuiFrames` probe). */
export const RENDERERS = [`react`, `solid`] as const
export type Renderer = (typeof RENDERERS)[number]
const HARNESS: Record<Renderer, string> = {
  react: join(import.meta.dir, `..`, `..`, `exponential-ui-react`, `harness`, `serve.ts`),
  solid: join(import.meta.dir, `..`, `..`, `exponential-ui-solid`, `harness`, `serve.ts`),
}

declare global {
  interface Window {
    /** The harnesses' box probe (packages/exponential-ui-{react,solid}/harness/main.tsx). */
    __xuiFrames?: (rootId?: string) => Frame[]
  }
}

export interface Frame {
  id: string
  x: number
  y: number
  w: number
  h: number
}

/** What one renderer painted at one width. */
export interface RendererMeasure {
  /** The painted root's size. */
  height: number
  /** Painted nodes. */
  nodes: number
  /** Vertical gaps between the root's direct children (px), in order. */
  gaps: number[]
  /** The distinct gap values: a short list = a rhythm. */
  distinctGaps: number[]
  /** Left inset of the first text run from the root's edge (px). */
  textInset: number | null
  /** Any node wider than the viewport. */
  overflowsX: boolean
  picture: string
}

/** One surface at one width: both renderers' measures and their comparison. */
export interface RenderMeasure {
  width: number
  react: RendererMeasure
  solid: RendererMeasure
  /** Both renderers measured the same boxes and painted the same pixels. */
  equal: boolean
  /** What differed (empty when `equal`): a measure by name, or the pixel count. */
  differences: string[]
  /** Pixels that differ between the two full-page pictures (threshold 0, anti-aliasing excluded). */
  pixelDiff: { pixels: number; share: number }
}

/** Every painted node's box → the measures above. `children` = the root's
 *  direct children in the normalized tree (ids), so the gaps are between
 *  what the author placed, not between every painted part. */
export function measureFrames(frames: Frame[], root: UiNode, width: number): Omit<RendererMeasure, `picture`> {
  const byId = new Map(frames.map((f) => [f.id, f]))
  const rootFrame = byId.get(root.id) ?? byId.get(`root`)
  const kids = root.children.map((c) => byId.get(c.id)).filter((f): f is Frame => !!f && f.h > 0)
  kids.sort((a, b) => a.y - b.y)
  const gaps: number[] = []
  for (let i = 1; i < kids.length; i++) gaps.push(Math.round(kids[i].y - (kids[i - 1].y + kids[i - 1].h)))
  const firstText = textNode(root)
  const textFrame = firstText ? byId.get(firstText.id) : undefined
  return {
    height: Math.round(rootFrame?.h ?? 0),
    nodes: frames.length,
    gaps,
    distinctGaps: [...new Set(gaps)].sort((a, b) => a - b),
    textInset: textFrame && rootFrame ? Math.round(textFrame.x - rootFrame.x) : null,
    overflowsX: frames.some((f) => f.x + f.w > width + 1),
  }
}

/** The measures that must agree between the renderers (the picture paths differ by name). */
export function compareMeasures(react: Omit<RendererMeasure, `picture`>, solid: Omit<RendererMeasure, `picture`>): string[] {
  const out: string[] = []
  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b)
  for (const key of [`height`, `nodes`, `gaps`, `textInset`, `overflowsX`] as const)
    if (!same(react[key], solid[key])) out.push(`${key}: react ${JSON.stringify(react[key])}, solid ${JSON.stringify(solid[key])}`)
  return out
}

function textNode(node: UiNode): UiNode | undefined {
  if (node.component === `Text`) return node
  for (const c of node.children) {
    const t = textNode(c)
    if (t) return t
  }
  return undefined
}

async function serveHarness(renderer: Renderer): Promise<{ base: string; stop: () => void }> {
  const port = 4700 + Math.floor(Math.random() * 300) + (renderer === `solid` ? 300 : 0)
  const proc = spawn(`bun`, [HARNESS[renderer]], { env: { ...process.env, PORT: String(port) }, stdio: [`ignore`, `ignore`, `inherit`] })
  const base = `http://127.0.0.1:${port}`
  // The Solid harness bundles with Vite on its first request: give it a minute.
  for (let i = 0; i < 400; i++) {
    try {
      if ((await fetch(`${base}/dist/main.js`)).ok) return { base, stop: () => proc.kill() }
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 150))
  }
  proc.kill()
  throw new Error(`the ${renderer} harness did not come up on :${port}`)
}

/** The flat components + data → the harness `?view=tree` query (a nested
 *  tree, the data model). */
function treeQuery(components: FlatComponent[], data: Record<string, unknown>, width: number): string {
  const { root, templates } = reduceSurface(components, { catalogId: `https://ui.exponential.at/catalogs/core/v1`, expand: false })
  const nested = JSON.stringify(withTemplates(root, templates ?? {}))
  return `?view=tree&tree=${encodeURIComponent(nested)}&data=${encodeURIComponent(JSON.stringify(data))}&width=${width}&theme=${theme}&mode=${mode}`
}

/** The harness takes ONE nested tree: template nodes ride along under
 *  their owner as `template` children (what reduceNested lifts back out). */
function withTemplates(node: UiNode, templates: Record<string, UiNode>): unknown {
  const out: Record<string, unknown> = { id: node.id, component: node.component, props: node.props }
  if (node.style) out.style = node.style
  if (node.on) out.on = node.on
  if (node.visible !== undefined) out.visible = node.visible
  if (node.accessibility) out.accessibility = node.accessibility
  if (node.template) {
    out.template = node.template
    const t = templates[node.template.component]
    if (t) out.children = [withTemplates(t, templates)]
  } else if (node.children.length) out.children = node.children.map((c) => withTemplates(c, templates))
  if (node.slots) out.slots = Object.fromEntries(Object.entries(node.slots).map(([k, v]) => [k, withTemplates(v, templates)]))
  return out
}

function dataModel(writes: { path: string; value: unknown }[]): Record<string, unknown> {
  const data: Record<string, unknown> = {}
  for (const w of writes) {
    const parts = w.path.split(`/`).filter(Boolean)
    if (!parts.length) {
      Object.assign(data, w.value as Record<string, unknown>)
      continue
    }
    let cursor: Record<string, unknown> = data
    for (const p of parts.slice(0, -1)) cursor = (cursor[p] ??= {}) as Record<string, unknown>
    cursor[parts[parts.length - 1]] = w.value
  }
  return data
}

/** What conformance/parity.ts injects before a picture: a spinner's frame or a blinking caret is no difference. */
const STILL_CSS = `*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}`

/** One renderer paints one surface at one width: the frames, the picture, the page errors. */
async function paint(browser: Browser, base: string, query: string, width: number, picture: string): Promise<{ frames: Frame[]; png: Buffer; errors: string[] }> {
  const context = await browser.newContext({ viewport: { width, height: 900 }, deviceScaleFactor: 2 })
  const page = await context.newPage()
  const errors: string[] = []
  page.on(`pageerror`, (e) => errors.push(e.message))
  await page.goto(`${base}/${query}`)
  await page.waitForSelector(`[data-xui-id]`, { timeout: 15_000 }).catch(() => undefined)
  // Still, as the web-parity gate paints: no animation frame or caret may differ between the renderers.
  await page.addStyleTag({ content: STILL_CSS })
  await page.waitForTimeout(500)
  const frames = (await page.evaluate(() => window.__xuiFrames?.(`root`) ?? [])) as Frame[]
  const png = await page.screenshot({ type: `png`, fullPage: true, animations: `disabled`, caret: `hide` })
  writeFileSync(picture, png)
  await context.close()
  return { frames, png, errors }
}

/** pixelmatch over the two pictures (threshold 0, anti-aliasing excluded, as the web-parity gate); a size difference counts every pixel outside the common area. */
export async function pixelDiff(a: Buffer, b: Buffer): Promise<{ pixels: number; share: number }> {
  const ra = await sharp(a).ensureAlpha().raw().toBuffer({ resolveWithObject: true })
  const rb = await sharp(b).ensureAlpha().raw().toBuffer({ resolveWithObject: true })
  const w = Math.min(ra.info.width, rb.info.width)
  const h = Math.min(ra.info.height, rb.info.height)
  const crop = (r: typeof ra) => (r.info.width === w && r.info.height === h ? r.data : sharp(r.data, { raw: { width: r.info.width, height: r.info.height, channels: 4 } }).extract({ left: 0, top: 0, width: w, height: h }).raw().toBuffer())
  const [ca, cb] = await Promise.all([crop(ra), crop(rb)])
  const inside = pixelmatch(new Uint8Array(ca), new Uint8Array(cb), undefined, w, h, { threshold: 0, includeAA: false })
  const total = Math.max(ra.info.width * ra.info.height, rb.info.width * rb.info.height)
  const pixels = inside + (total - w * h)
  return { pixels, share: total ? pixels / total : 0 }
}

async function main(): Promise<void> {
  const entries = (corpus as Corpus).prompts.filter((p) => !only.size || only.has(p.id))
  const servers = { react: await serveHarness(`react`), solid: await serveHarness(`solid`) }
  const browser = await chromium.launch()
  const render: Record<string, RenderMeasure[]> = (readReport()?.render as Record<string, RenderMeasure[]> | undefined) ?? {}
  let findings = 0
  try {
    for (const entry of entries) {
      const fixture = readFixture(entry.id)
      if (!fixture) {
        console.warn(`${entry.id}: no fixture`)
        continue
      }
      const surface = surfaceOf(parseReply(fixture.text).messages)
      const data = dataModel(surface.data)
      const dir = join(OUT, entry.id)
      mkdirSync(dir, { recursive: true })
      const { root } = reduceSurface(surface.components, { catalogId: surface.catalogId })
      const measures: RenderMeasure[] = []
      for (const width of widthsOf(entry)) {
        const query = treeQuery(surface.components, data, width)
        const painted = {} as Record<Renderer, { measure: RendererMeasure; png: Buffer; errors: string[] }>
        for (const renderer of RENDERERS) {
          const relative = `eval/out/${entry.id}/${renderer}-${width}.png`
          const { frames, png, errors } = await paint(browser, servers[renderer].base, query, width, join(dir, `${renderer}-${width}.png`))
          painted[renderer] = { measure: { ...measureFrames(frames, root, width), picture: relative }, png, errors }
        }
        const differences = compareMeasures(painted.react.measure, painted.solid.measure)
        const pixels = await pixelDiff(painted.react.png, painted.solid.png)
        if (pixels.pixels) differences.push(`pixels: ${pixels.pixels} differ (${(pixels.share * 100).toFixed(2)} %)`)
        for (const renderer of RENDERERS) for (const e of painted[renderer].errors) differences.push(`${renderer} page error: ${e}`)
        const measure: RenderMeasure = { width, react: painted.react.measure, solid: painted.solid.measure, equal: differences.length === 0, differences, pixelDiff: pixels }
        measures.push(measure)
        if (!measure.equal) findings++
        const m = measure.react
        console.log(`${entry.id} @${width}: ${m.nodes} nodes, ${m.height}px tall, gaps ${m.distinctGaps.join(`/`) || `–`}, inset ${m.textInset ?? `–`}${m.overflowsX ? `, OVERFLOWS` : ``}; solid ${measure.equal ? `equal` : `DIFFERS: ${differences.join(`; `)}`}`)
      }
      render[entry.id] = measures
    }
  } finally {
    await browser.close()
    servers.react.stop()
    servers.solid.stop()
  }
  const report = buildReport({ ...readReport(), render })
  writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`)
  console.log(`→ ${REPORT_PATH} (render: ${Object.keys(render).length} surfaces × ${RENDERERS.join(` + `)}; ${findings ? `${findings} width(s) where Solid differs from React` : `Solid equals React everywhere`})`)
}

if (import.meta.main) main().catch((e) => {
  console.error(e)
  process.exit(1)
})
