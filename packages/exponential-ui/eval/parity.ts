#!/usr/bin/env bun
// VAPP-108: the Devices template (packages/ui/exponential-ui/templates/
// devices.json, view `exponential-ui-devices`) against the native Devices
// page (view `agents`) on the stored shots, web AND desktop: the geometry
// both screens share, measured in pixels, and a pixel diff of the band they
// both draw. The web twin is the existing check; this adds the desktop
// (gpui) twin and records both in eval/report.json `parity`.
//
//   bun eval/parity.ts
//
// What is measured (store px; the store scales a 1440-wide @2x capture to
// 1800, so 1 css px = 1.25 store px):
//   column      the content column's left and right edge (first / last
//               column holding non-background pixels in the page body)
//   band        the Section / "My devices" header band: its top, height and
//               the x where its title text starts (the band's left inset)
//   firstRow    the first device row's title: its top and left x, and the
//               distance from the band's bottom
//   pixelDiff   pixelmatch over the band region, both shots cropped to the
//               same box (threshold 0.1), as a share of the pixels
//
// The native page and the template differ on purpose below the band (the
// native lists every device's logins), so the comparison stops at the first
// row's title; eval/parity.test.ts holds the tolerances.

import pixelmatch from "pixelmatch"
import sharp from "sharp"
import { existsSync, readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import corpus from "./corpus.json" with { type: "json" }
import { widthsOf, type Corpus } from "./lib"
import { REPORT_PATH, buildReport, readReport } from "./score"

const ROOT = join(import.meta.dir, `..`, `..`, `..`)
const SHOTS = join(ROOT, `shots`)
/** store px per css px for the 1440-wide frames (web 1440×960 @2x, desktop 1440×900 @2x → 1800 wide). */
export const STORE_SCALE = 1.25

export type Platform = `web` | `desktop`
export interface Raw {
  data: Uint8Array
  width: number
  height: number
}

async function raw(file: string): Promise<Raw> {
  const { data, info } = await sharp(readFileSync(file)).ensureAlpha().raw().toBuffer({ resolveWithObject: true })
  return { data, width: info.width, height: info.height }
}

const lum = (img: Raw, x: number, y: number) => {
  const i = (y * img.width + x) * 4
  return 0.2126 * img.data[i] + 0.7152 * img.data[i + 1] + 0.0722 * img.data[i + 2]
}

/** The page body's background: the most common luminance along the row at
 *  60 % of the height (the content area, below any header), to the nearest
 *  unit. */
function background(img: Raw, x0: number, x1: number): number {
  const y = Math.floor(img.height * 0.6)
  const counts = new Map<number, number>()
  for (let x = x0; x < x1; x++) {
    const l = Math.round(lum(img, x, y))
    counts.set(l, (counts.get(l) ?? 0) + 1)
  }
  return [...counts.entries()].sort((a, b) => b[1] - a[1])[0][0]
}

export interface Geometry {
  /** The app chrome's width on the left (desktop: the sidebar; web: the sidebar), the content area starts after it. */
  contentLeft: number
  column: { left: number; right: number }
  band: { top: number; height: number; titleX: number }
  firstRow: { titleTop: number; titleX: number; gapBelowBand: number }
}

/** A row is "ink" when enough of its pixels in [x0, x1) differ from the
 *  background by more than `delta`. */
function inkRows(img: Raw, x0: number, x1: number, y0: number, y1: number, bg: number, delta = 10, minPixels = 3): boolean[] {
  const rows: boolean[] = []
  for (let y = y0; y < y1; y++) {
    let n = 0
    for (let x = x0; x < x1; x++) if (Math.abs(lum(img, x, y) - bg) > delta && ++n >= minPixels) break
    rows.push(n >= minPixels)
  }
  return rows
}

/** Runs of consecutive `true` rows as [start, end) pairs (absolute y). */
function runs(flags: boolean[], offset: number): [number, number][] {
  const out: [number, number][] = []
  let start = -1
  flags.forEach((f, i) => {
    if (f && start < 0) start = i
    if (!f && start >= 0) {
      out.push([start + offset, i + offset])
      start = -1
    }
  })
  if (start >= 0) out.push([start + offset, flags.length + offset])
  return out
}

/** First x in [x0, x1) whose column holds ink within rows [y0, y1). */
function firstInkX(img: Raw, x0: number, x1: number, y0: number, y1: number, bg: number, delta = 40): number {
  for (let x = x0; x < x1; x++) for (let y = y0; y < y1; y++) if (Math.abs(lum(img, x, y) - bg) > delta) return x
  return -1
}

/** The content area: the sidebar ends where a long run of background-only
 *  columns begins (the gutter between the rail and the page). */
function contentLeft(img: Raw, bg: number): number {
  const y0 = Math.floor(img.height * 0.3)
  const y1 = Math.floor(img.height * 0.9)
  let run = 0
  for (let x = 0; x < img.width; x++) {
    let ink = false
    for (let y = y0; y < y1; y += 2) if (Math.abs(lum(img, x, y) - bg) > 10) {
      ink = true
      break
    }
    run = ink ? 0 : run + 1
    if (run >= 24 && x > 100) return x - run + 1
  }
  return 0
}

/** The longest run of FLAT tint on row y (pixels within ±3 of each other,
 *  differing from the page background by 3..60: a band fill, not text or
 *  a hairline). */
function tintRun(img: Raw, y: number, x0: number, x1: number, bg: number): [number, number] {
  let best: [number, number] = [x0, x0]
  let start = -1
  let ref = 0
  for (let x = x0; x <= x1; x++) {
    const l = x < x1 ? lum(img, x, y) : Number.NaN
    const d = Math.abs(l - bg)
    const tint = d >= 3 && d <= 60
    if (tint && (start < 0 || Math.abs(l - ref) <= 3)) {
      if (start < 0) {
        start = x
        ref = l
      }
      continue
    }
    if (start >= 0 && x - start > best[1] - best[0]) best = [start, x]
    start = tint ? x : -1
    ref = tint ? l : 0
  }
  return best
}

/** The band = the first run of ≥ 30 rows (from `top`, a fraction of the
 *  height: skips the window chrome) whose flat tint spans ≥ 35 % of the
 *  content width; its x extent is the median of those rows' runs. The
 *  column IS the band's extent; the first row's title is the first text
 *  run under the band inside it. */
export function measure(img: Raw, options: { top?: number } = {}): Geometry {
  const left = contentLeft(img, background(img, Math.floor(img.width * 0.4), img.width))
  const bg = background(img, left, img.width)
  const top = Math.floor(img.height * (options.top ?? 0.02))
  const half = Math.floor(img.height * 0.5)
  const minWidth = (img.width - left) * 0.35
  const runsByRow: [number, number][] = []
  for (let y = top; y < half; y++) {
    const r = tintRun(img, y, left, img.width, bg)
    runsByRow.push(r[1] - r[0] >= minWidth ? r : [0, 0])
  }
  const band = runs(runsByRow.map((r) => r[1] > r[0]), top).find(([a, b]) => b - a >= 30) ?? [top, top]
  const midY = Math.floor((band[0] + band[1]) / 2)
  const midRun = runsByRow[midY - top] ?? [left, left]
  // The band's tint = the flat run's luminance on its middle row; its extent
  // = the first and last pixel of that tint on the row (text inside the band
  // breaks the run but not the extent).
  const bandBg = Math.round(lum(img, Math.floor((midRun[0] + midRun[1]) / 2), midY))
  // Walk outward from the flat run on a row ABOVE the glyphs (text sits in
  // the band's middle) while the tint holds (±2: the enclosing card's fill
  // differs by more): the band's full extent, text and all.
  const yTop = band[0] + Math.min(6, Math.floor((band[1] - band[0]) / 4))
  let colLeft = midRun[0]
  let colRight = midRun[1]
  while (colLeft > left && Math.abs(lum(img, colLeft - 1, yTop) - bandBg) <= 2) colLeft--
  while (colRight < img.width && Math.abs(lum(img, colRight, yTop) - bandBg) <= 2) colRight++
  const titleX = firstInkX(img, colLeft, colRight, band[0] + 4, band[1] - 4, bandBg)
  // Below the band: the first text run (ink rows against the page background).
  const below = inkRows(img, colLeft, colRight, band[1] + 2, half, bg, 30)
  const rowRuns = runs(below, band[1] + 2).filter(([a, b]) => b - a >= 8)
  const first = rowRuns[0] ?? [band[1], band[1]]
  const rowTitleX = firstInkX(img, colLeft, colRight, first[0], first[1], bg, 60)
  return {
    contentLeft: left,
    column: { left: colLeft, right: colRight },
    band: { top: band[0], height: band[1] - band[0], titleX: titleX - colLeft },
    firstRow: { titleTop: first[0], titleX: rowTitleX - colLeft, gapBelowBand: first[0] - band[1] },
  }
}

export interface ParityRow {
  platform: Platform
  native: Geometry
  template: Geometry
  /** template − native, store px. */
  delta: { columnWidth: number; bandHeight: number; bandTitleX: number; rowTitleX: number; gapBelowBand: number }
  /** pixelmatch over the band (both cropped to the native band's box, aligned at each one's column-left / band-top). */
  bandDiff: { pixels: number; total: number; share: number }
}

async function bandDiff(a: Raw, b: Raw, ga: Geometry, gb: Geometry): Promise<ParityRow[`bandDiff`]> {
  const w = Math.min(ga.column.right - ga.column.left, gb.column.right - gb.column.left)
  const h = Math.min(ga.band.height, gb.band.height)
  const crop = async (img: Raw, g: Geometry) => (await sharp(Buffer.from(img.data), { raw: { width: img.width, height: img.height, channels: 4 } }).extract({ left: g.column.left, top: g.band.top, width: w, height: h }).raw().toBuffer()) as Uint8Array
  const ca = await crop(a, ga)
  const cb = await crop(b, gb)
  const pixels = pixelmatch(ca, cb, undefined, w, h, { threshold: 0.1 })
  return { pixels, total: w * h, share: Math.round((pixels / (w * h)) * 10000) / 10000 }
}

export async function compare(platform: Platform): Promise<ParityRow> {
  const native = await raw(join(SHOTS, `agents`, `${platform}.webp`))
  const template = await raw(join(SHOTS, `exponential-ui-devices`, `${platform}.webp`))
  // The desktop shots start with the window's tab strip (≈ 50 store px).
  const opts = { top: platform === `desktop` ? 0.06 : 0.02 }
  const gn = measure(native, opts)
  const gt = measure(template, opts)
  return {
    platform,
    native: gn,
    template: gt,
    delta: {
      columnWidth: gt.column.right - gt.column.left - (gn.column.right - gn.column.left),
      bandHeight: gt.band.height - gn.band.height,
      bandTitleX: gt.band.titleX - gn.band.titleX,
      rowTitleX: gt.firstRow.titleX - gn.firstRow.titleX,
      gapBelowBand: gt.firstRow.gapBelowBand - gn.firstRow.gapBelowBand,
    },
    bandDiff: await bandDiff(native, template, gn, gt),
  }
}

export const css = (storePx: number) => Math.round((storePx / STORE_SCALE) * 10) / 10

export function describe(row: ParityRow): string {
  const d = row.delta
  return [
    `${row.platform}: column ${row.native.column.right - row.native.column.left} vs ${row.template.column.right - row.template.column.left} store px (Δ ${d.columnWidth} = ${css(d.columnWidth)} css)`,
    `  band height ${row.native.band.height} vs ${row.template.band.height} (Δ ${d.bandHeight} = ${css(d.bandHeight)} css); title inset ${row.native.band.titleX} vs ${row.template.band.titleX} (Δ ${d.bandTitleX} = ${css(d.bandTitleX)} css)`,
    `  first row title inset ${row.native.firstRow.titleX} vs ${row.template.firstRow.titleX} (Δ ${d.rowTitleX} = ${css(d.rowTitleX)} css); gap below band ${row.native.firstRow.gapBelowBand} vs ${row.template.firstRow.gapBelowBand} (Δ ${d.gapBelowBand} = ${css(d.gapBelowBand)} css)`,
    `  band pixel diff ${row.bandDiff.pixels} / ${row.bandDiff.total} (${(row.bandDiff.share * 100).toFixed(1)} %)`,
  ].join(`\n`)
}

// ---------------------------------------------------------------------------
// VAPP-127: every corpus surface that names an `appView` against that app
// screen, on every platform where both pictures exist: the eval's own render
// (web: eval/out/<id>/react-<w>.png from render.ts; android / ios / desktop:
// eval/out/<id>/<platform>.webp from the shots pipeline's eval lane) and the
// stored app shot (shots/<appView>/<platform>.webp). The screens differ in
// content on purpose (an agent's surface is not the app's screen), so what is
// compared is the SPACING both draw, measured the same way on both, in css px:
// the left inset of the first ink from the content's edge, the top inset,
// and the vertical rhythm (the gaps between successive runs of ink rows: the
// median and the most common values). No gate: a record, like the Devices
// rows' first run was.
// ---------------------------------------------------------------------------

export type SurfacePlatform = Platform | `android` | `ios`
export const SURFACE_PLATFORMS: readonly SurfacePlatform[] = [`web`, `desktop`, `android`, `ios`]
/** The phones' css width the stored captures span (the Medium Phone AVD's 412 dp, the iPhone 17 Pro Max's 440 pt). */
const PHONE_CSS_WIDTH = { android: 412, ios: 440 } as const

export interface Spacing {
  /** Where the content starts (the app chrome's width), css px. */
  contentLeft: number
  /** The first ink's left inset from the content's edge. */
  inset: number
  /** The first ink's top inset from the measured region's top. */
  top: number
  /** Runs of ink rows (text lines, controls, rules). */
  runs: number
  medianGap: number
  /** The most common gaps (to 2 css px), most frequent first. */
  gaps: number[]
}

export interface SurfaceParityRow {
  surface: string
  appView: string
  platform: SurfacePlatform
  template: Spacing
  native: Spacing
  /** template − native, css px. */
  delta: { inset: number; top: number; medianGap: number }
  /** Share of the template's common gaps the app screen also uses (±2 css px). */
  sharedGaps: number
  pictures: { template: string; native: string }
}

const round = (v: number) => Math.round(v * 10) / 10

/** The spacing of one picture (`cssPerPx` = css px per image px; `top` = the
 *  fraction of the height the status bar / tab strip takes; `chrome` = find
 *  the app's sidebar first). */
export function spacing(img: Raw, cssPerPx: number, options: { top: number; chrome: boolean }): Spacing {
  const bg0 = background(img, Math.floor(img.width * 0.4), img.width)
  const left = options.chrome ? contentLeft(img, bg0) : 0
  const bg = background(img, left, img.width)
  const y0 = Math.floor(img.height * options.top)
  const y1 = Math.floor(img.height * 0.95)
  const px = (css: number) => Math.max(1, Math.round(css / cssPerPx))
  // Ink runs, the ones closer than 2 css px merged, the ones thinner than 2 css px dropped.
  // Text-contrast ink only (Δ > 40): a card's fill or a hairline (≈ 5..20 off the page) is not a line.
  const raw = runs(inkRows(img, left, img.width, y0, y1, bg, 40), y0)
  const merged: [number, number][] = []
  for (const r of raw) {
    const last = merged[merged.length - 1]
    if (last && r[0] - last[1] < px(2)) last[1] = r[1]
    else merged.push([r[0], r[1]])
  }
  const ink = merged.filter(([a, b]) => b - a >= px(2))
  const gaps = ink.slice(1).map((r, i) => (r[0] - ink[i][1]) * cssPerPx)
  const sorted = [...gaps].sort((a, b) => a - b)
  const median = sorted.length ? sorted[Math.floor(sorted.length / 2)] : 0
  const counts = new Map<number, number>()
  for (const g of gaps) counts.set(Math.round(g / 2) * 2, (counts.get(Math.round(g / 2) * 2) ?? 0) + 1)
  const insets = ink.slice(0, 8).map(([a, b]) => firstInkX(img, left, img.width, a, b, bg, 40)).filter((x) => x >= 0)
  return {
    contentLeft: round(left * cssPerPx),
    inset: insets.length ? round((Math.min(...insets) - left) * cssPerPx) : 0,
    top: ink.length ? round((ink[0][0] - y0) * cssPerPx) : 0,
    runs: ink.length,
    medianGap: round(median),
    gaps: [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0] - b[0]).slice(0, 6).map(([g]) => g),
  }
}

/** The eval's own picture of `id` on `platform`, if one was taken. */
function templatePicture(id: string, platform: SurfacePlatform, widths: number[]): string | undefined {
  const dir = join(import.meta.dir, `out`, id)
  if (platform !== `web`) return existsSync(join(dir, `${platform}.webp`)) ? join(dir, `${platform}.webp`) : undefined
  const w = widths.includes(1280) ? 1280 : Math.max(...widths)
  return existsSync(join(dir, `react-${w}.png`)) ? join(dir, `react-${w}.png`) : undefined
}

export async function compareSurfaces(): Promise<SurfaceParityRow[]> {
  const rows: SurfaceParityRow[] = []
  for (const entry of (corpus as Corpus).prompts) {
    if (!entry.appView) continue
    for (const platform of SURFACE_PLATFORMS) {
      const t = templatePicture(entry.id, platform, widthsOf(entry))
      const n = join(SHOTS, entry.appView, `${platform}.webp`)
      if (!t || !existsSync(n)) continue
      const template = await raw(t)
      const native = await raw(n)
      const phone = platform === `android` || platform === `ios`
      const scale = (img: Raw, isTemplate: boolean) =>
        phone ? PHONE_CSS_WIDTH[platform as `android` | `ios`] / img.width : platform === `web` && isTemplate ? 0.5 : 1 / STORE_SCALE
      const top = platform === `web` ? (0.02) : 0.06
      const st = spacing(template, scale(template, true), { top: platform === `web` ? 0 : top, chrome: platform === `desktop` })
      const sn = spacing(native, scale(native, false), { top, chrome: platform === `web` || platform === `desktop` })
      const shared = st.gaps.length ? st.gaps.filter((g) => sn.gaps.some((h) => Math.abs(g - h) <= 2)).length / st.gaps.length : 0
      rows.push({
        surface: entry.id,
        appView: entry.appView,
        platform,
        template: st,
        native: sn,
        delta: { inset: round(st.inset - sn.inset), top: round(st.top - sn.top), medianGap: round(st.medianGap - sn.medianGap) },
        sharedGaps: Math.round(shared * 100) / 100,
        pictures: { template: t.slice(ROOT.length + 1), native: n.slice(ROOT.length + 1) },
      })
    }
  }
  return rows
}

export function describeSurface(row: SurfaceParityRow): string {
  return `${row.surface} vs ${row.appView} (${row.platform}): inset ${row.template.inset} vs ${row.native.inset} (Δ ${row.delta.inset}), median gap ${row.template.medianGap} vs ${row.native.medianGap} (Δ ${row.delta.medianGap}), gaps ${row.template.gaps.join(`/`)} vs ${row.native.gaps.join(`/`)} (${Math.round(row.sharedGaps * 100)} % shared) css px`
}

/** VAPP-128: the Section macro's own rhythm on its specimen shot (view
 *  `exponential-ui-section`, the first case: a titled band over tree rows):
 *  the band and the gap from its bottom to the first row's title, store px. */
export interface SectionBand {
  platform: Platform
  band: Geometry[`band`]
  gapBelowBand: number
}

export async function sectionBand(platform: Platform): Promise<SectionBand> {
  const g = measure(await raw(join(SHOTS, `exponential-ui-section`, `${platform}.webp`)), { top: platform === `desktop` ? 0.06 : 0.02 })
  return { platform, band: g.band, gapBelowBand: g.firstRow.gapBelowBand }
}

if (import.meta.main) {
  // VAPP-128: the template's own view (exponential-ui-devices) left the store
  // with its dev routes in VAPP-111 (the template stays the SDK's example
  // package; the product ships no built-in apps); without its shot a platform's row stays
  // the one on record.
  const recorded = ((readReport() as { parity?: { rows?: ParityRow[] } }).parity?.rows ?? []) as ParityRow[]
  const rows: ParityRow[] = []
  for (const platform of [`web`, `desktop`] as const) {
    if (existsSync(join(SHOTS, `exponential-ui-devices`, `${platform}.webp`))) rows.push(await compare(platform))
    else {
      const kept = recorded.find((r) => r.platform === platform)
      console.log(`${platform}: no exponential-ui-devices shot; the row on record stays`)
      if (kept) rows.push(kept)
    }
  }
  for (const row of rows) console.log(describe(row))
  const surfaces = await compareSurfaces()
  for (const row of surfaces) console.log(describeSurface(row))
  const section = [await sectionBand(`web`), await sectionBand(`desktop`)]
  for (const s of section) console.log(`${s.platform}: Section specimen band ${s.band.height} tall, gap below band ${s.gapBelowBand} store px (${css(s.gapBelowBand)} css)`)
  const report = buildReport({ ...readReport(), parity: { $comment: `Devices template (exponential-ui-devices) vs the native Devices page (agents), on the stored shots; store px (1 css px = ${STORE_SCALE} store px). \`surfaces\` (VAPP-127): every corpus surface naming an appView vs that app screen's stored shot, per platform where the eval has a picture (web: render.ts; android / ios / desktop: the shots pipeline's eval lane), the spacing both draw in css px. VAPP-128: \`section\` = the Section macro's band and the gap under it on its specimen shot (exponential-ui-section).`, rows, surfaces, section } })
  writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`)
  console.log(`→ ${REPORT_PATH}`)
}
