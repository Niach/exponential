#!/usr/bin/env bun
// VAPP-108: renders every recorded surface on the web (the React harness in
// headless Chromium, the exponential theme, dark) at each corpus width,
// writes the pictures to eval/out/<id>/web-<width>.png (gitignored) and
// MEASURES the painted boxes (__xuiFrames): the vertical gaps between the
// root's children, the left inset of the first text, the painted height,
// whether anything overflows the viewport sideways. The measurements go
// into eval/report.json `render` (committed) so the numbers are on record
// without the pictures.
//
//   bun eval/render.ts [--only <id,id>] [--theme exponential] [--mode dark]
//
// The natives go through the shots pipeline's specimen lane once a corpus
// surface is a view (follow-up on VAPP-108); this is the stack-free door.

import { chromium } from "playwright"
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
const HARNESS = join(import.meta.dir, `..`, `..`, `exponential-ui-react`, `harness`, `serve.ts`)

declare global {
  interface Window {
    /** The React harness's box probe (packages/exponential-ui-react/harness/main.tsx). */
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

export interface RenderMeasure {
  width: number
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

/** Every painted node's box → the measures above. `children` = the root's
 *  direct children in the normalized tree (ids), so the gaps are between
 *  what the author placed, not between every painted part. */
export function measureFrames(frames: Frame[], root: UiNode, width: number): Omit<RenderMeasure, `picture`> {
  const byId = new Map(frames.map((f) => [f.id, f]))
  const rootFrame = byId.get(root.id) ?? byId.get(`root`)
  const kids = root.children.map((c) => byId.get(c.id)).filter((f): f is Frame => !!f && f.h > 0)
  kids.sort((a, b) => a.y - b.y)
  const gaps: number[] = []
  for (let i = 1; i < kids.length; i++) gaps.push(Math.round(kids[i].y - (kids[i - 1].y + kids[i - 1].h)))
  const firstText = textNode(root)
  const textFrame = firstText ? byId.get(firstText.id) : undefined
  return {
    width,
    height: Math.round(rootFrame?.h ?? 0),
    nodes: frames.length,
    gaps,
    distinctGaps: [...new Set(gaps)].sort((a, b) => a - b),
    textInset: textFrame && rootFrame ? Math.round(textFrame.x - rootFrame.x) : null,
    overflowsX: frames.some((f) => f.x + f.w > width + 1),
  }
}

function textNode(node: UiNode): UiNode | undefined {
  if (node.component === `Text`) return node
  for (const c of node.children) {
    const t = textNode(c)
    if (t) return t
  }
  return undefined
}

async function serveHarness(): Promise<{ base: string; stop: () => void }> {
  const port = 4700 + Math.floor(Math.random() * 300)
  const proc = spawn(`bun`, [HARNESS], { env: { ...process.env, PORT: String(port) }, stdio: [`ignore`, `ignore`, `inherit`] })
  const base = `http://127.0.0.1:${port}`
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`${base}/dist/main.js`)).ok) return { base, stop: () => proc.kill() }
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 150))
  }
  proc.kill()
  throw new Error(`the harness did not come up on :${port}`)
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

async function main(): Promise<void> {
  const entries = (corpus as Corpus).prompts.filter((p) => !only.size || only.has(p.id))
  const { base, stop } = await serveHarness()
  const browser = await chromium.launch()
  const render: Record<string, RenderMeasure[]> = (readReport()?.render as Record<string, RenderMeasure[]> | undefined) ?? {}
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
      const measures: RenderMeasure[] = []
      for (const width of widthsOf(entry)) {
        const context = await browser.newContext({ viewport: { width, height: 900 }, deviceScaleFactor: 2 })
        const page = await context.newPage()
        const errors: string[] = []
        page.on(`pageerror`, (e) => errors.push(e.message))
        await page.goto(`${base}/${treeQuery(surface.components, data, width)}`)
        await page.waitForSelector(`[data-xui-id]`, { timeout: 15_000 }).catch(() => undefined)
        await page.waitForTimeout(500)
        const frames = (await page.evaluate(() => window.__xuiFrames?.(`root`) ?? [])) as Frame[]
        const picture = join(dir, `web-${width}.png`)
        writeFileSync(picture, await page.screenshot({ type: `png`, fullPage: true }))
        const { root } = reduceSurface(surface.components, { catalogId: surface.catalogId })
        const measure = { ...measureFrames(frames, root, width), picture: `eval/out/${entry.id}/web-${width}.png` }
        measures.push(measure)
        console.log(`${entry.id} @${width}: ${measure.nodes} nodes, ${measure.height}px tall, gaps ${measure.distinctGaps.join(`/`) || `–`}, inset ${measure.textInset ?? `–`}${measure.overflowsX ? `, OVERFLOWS` : ``}${errors.length ? `, errors: ${errors.join(`; `)}` : ``}`)
        await context.close()
      }
      render[entry.id] = measures
    }
  } finally {
    await browser.close()
    stop()
  }
  const report = buildReport({ ...readReport(), render })
  writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`)
  console.log(`→ ${REPORT_PATH} (render: ${Object.keys(render).length} surfaces)`)
}

if (import.meta.main) main().catch((e) => {
  console.error(e)
  process.exit(1)
})
