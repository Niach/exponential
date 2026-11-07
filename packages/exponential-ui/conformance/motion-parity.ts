// EXP-1604: the web Motion painter's pixel parity (eui-motion README.md
// › Painting a Motion §7). Every case of fixtures/motion-frames.json is
// painted by the shared Canvas2D painter (src/web/primitives/motion-paint.ts)
// in headless Chromium at 160×160 (contain, transparent) and compared with
// the raster's reference conformance/motion/parity/<case>@<frame>.png:
// ≤ 2/255 per premultiplied channel on ≥ 99 % of the pixels.

import { execFileSync } from "node:child_process"
import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import type { Page } from "playwright"
import { motionWasmSync } from "../src/motion/server"

export const MOTION_PARITY_SIZE = 160
export const MOTION_PARITY_TOLERANCE = 2
export const MOTION_PARITY_MIN_SHARE = 0.99

/** Cases under 99 % on Canvas2D, all on anti-aliased edges (Skia's analytic
 *  coverage vs tiny-skia's 4× supersampling) or bilinear sampling (a ½ px
 *  phase vs tiny-skia): their floor, a list that may only shrink. */
export function motionParityFloor(name: string): number {
  const gaps = JSON.parse(readFileSync(join(ROOT, `conformance`, `motion`, `web-parity-gaps.json`), `utf8`)) as { gaps: Record<string, { floor: number }> }
  return gaps.gaps[name]?.floor ?? MOTION_PARITY_MIN_SHARE
}

export type MotionParityCase = { id: string; frame: number; packed: number[]; images: string[]; reference: string }
export type MotionParityResult = { name: string; matched: number; total: number; share: number; maxDiff: number }

const ROOT = join(import.meta.dirname, `..`)

/** The cases: one per (fixture case, sampled frame), with the reference PNG as a data URL. */
export function motionParityCases(): MotionParityCase[] {
  const wasm = motionWasmSync()
  const fixture = JSON.parse(readFileSync(join(ROOT, `fixtures`, `motion-frames.json`), `utf8`)) as { cases: { id: string; lottie: unknown; frames: { frame: number; packed: number[] }[] }[] }
  const out: MotionParityCase[] = []
  for (const c of fixture.cases) {
    const player = new wasm.MotionPlayer(JSON.stringify(c.lottie), ``)
    const images = player.images()
    player.free()
    for (const f of c.frames) {
      const png = readFileSync(join(ROOT, `conformance`, `motion`, `parity`, `${c.id}@${f.frame}.png`))
      out.push({ id: c.id, frame: f.frame, packed: f.packed, images, reference: `data:image/png;base64,${png.toString(`base64`)}` })
    }
  }
  return out
}

/** The painter as a browser script defining `window.__motionPaint`. */
export async function motionPainterScript(): Promise<string> {
  // A `bun build` child: the browser suites run under vitest (Node workers).
  return execFileSync(`bun`, [`build`, join(ROOT, `conformance`, `motion-parity-entry.ts`), `--target=browser`, `--format=iife`], { encoding: `utf8`, maxBuffer: 64 << 20 })
}

/** Paint every case on `page` (a blank page) and compare. */
export async function runMotionParity(page: Page, cases = motionParityCases()): Promise<MotionParityResult[]> {
  await page.addScriptTag({ content: await motionPainterScript() })
  const results: MotionParityResult[] = []
  for (const c of cases) {
    const r = await page.evaluate(
      async ({ c, size, tol }) => {
        const P = (window as unknown as { __motionPaint: typeof import("../src/web/primitives/motion-paint") }).__motionPaint
        const load = (src: string) =>
          new Promise<HTMLImageElement | null>((res) => {
            const img = new Image()
            img.onload = () => res(img)
            img.onerror = () => res(null)
            img.src = src
          })
        const images = await Promise.all(c.images.map(load))
        const canvas = document.createElement(`canvas`)
        canvas.width = canvas.height = size
        const ctx = canvas.getContext(`2d`, { willReadFrequently: true })!
        const { width, height } = P.motionFrameSize(c.packed)
        P.paintMotionFrame(ctx, size, size, c.packed, P.motionFitMatrix(width, height, size, size, `contain`), images)
        const got = ctx.getImageData(0, 0, size, size).data
        const ref = await load(c.reference)
        const rc = document.createElement(`canvas`)
        rc.width = rc.height = size
        const rctx = rc.getContext(`2d`, { willReadFrequently: true })!
        rctx.drawImage(ref!, 0, 0)
        const want = rctx.getImageData(0, 0, size, size).data
        let matched = 0
        let maxDiff = 0
        const pm = (d: Uint8ClampedArray, k: number, ch: number) => (ch === 3 ? d[k + 3]! : (d[k + ch]! * d[k + 3]!) / 255)
        for (let k = 0; k < got.length; k += 4) {
          let worst = 0
          for (let ch = 0; ch < 4; ch++) worst = Math.max(worst, Math.abs(pm(got, k, ch) - pm(want, k, ch)))
          worst = Math.round(worst)
          maxDiff = Math.max(maxDiff, worst)
          if (worst <= tol) matched++
        }
        return { matched, total: got.length / 4, maxDiff, png: canvas.toDataURL() }
      },
      { c, size: MOTION_PARITY_SIZE, tol: MOTION_PARITY_TOLERANCE }
    )
    const { png, ...rest } = r
    if (process.env.MOTION_PARITY_DUMP) writeFileSync(join(process.env.MOTION_PARITY_DUMP, `${c.id}@${c.frame}.png`), Buffer.from(png.split(`,`)[1]!, `base64`))
    results.push({ name: `${c.id}@${c.frame}`, ...rest, share: rest.matched / rest.total })
  }
  return results
}

export type MotionFilterParityResult = { name: string; size: number; native: boolean; share: number; maxDiff: number }

/** The native filter path (`ctx.filter` blur, the drop shadow built on it)
 *  against the pixel ports it replaces, on synthetic layers whose content
 *  stays off the canvas edge: within 2/255 per premultiplied channel on
 *  ≥ 99 % of the pixels. `native` = the engine took the native path. */
export async function runMotionFilterParity(page: Page): Promise<MotionFilterParityResult[]> {
  if (!(await page.evaluate(() => `__motionPaint` in window))) await page.addScriptTag({ content: await motionPainterScript() })
  const matrix = [1, 0.3, 0.3, 0.3, 0, 0, 0.3, 0.3, 0.3, 0, 0, 0.3, 0.3, 0.3, 0, 0, 0, 0, 0, 1, 0]
  const cases: [string, number[][]][] = []
  for (const s of [0.5, 1.5, 2, 4, 8, 16]) cases.push([`blur ${s}`, [[0, s, s]]])
  cases.push([`blur 4×1 (anisotropic: the port)`, [[0, 4, 1]]])
  for (const s of [0, 1, 2, 4, 10]) for (const [dx, dy] of [[0, 0], [12, 7], [3.4, -5.7]]) cases.push([`shadow σ${s} ${dx},${dy}`, [[3, 0.1, 0.2, 0.3, 0.7, dx!, dy!, s]]])
  cases.push([`blur 4 then matrix`, [[0, 4, 4], [1, ...matrix]]])
  cases.push([`matrix then blur 4`, [[1, ...matrix], [0, 4, 4]]])
  return page.evaluate(
    ({ cases, tol }) => {
      const P = (window as unknown as { __motionPaint: typeof import("../src/web/primitives/motion-paint") }).__motionPaint
      const I = [1, 0, 0, 1, 0, 0]
      const rect = (x: number, y: number, w: number, h: number) => [5, 0, x, y, 1, x + w, y, 1, x + w, y + h, 1, x, y + h, 3]
      // A 200×200 Motion: one layer (the filters) over an opaque and a translucent rect.
      const frame = (filters: number[][]) => [1, 200, 200, 1, 0, 1, ...I, ...rect(0, 0, 200, 200), filters.length, ...filters.flat(), 4, ...I, 0, 0, 0.9, 0.3, 0.1, 1, ...rect(60, 50, 70, 90), 4, ...I, 0, 0, 0.1, 0.4, 0.9, 0.6, ...rect(100, 100, 40, 30), 3]
      const out: { name: string; size: number; native: boolean; share: number; maxDiff: number }[] = []
      for (const size of [200, 512])
        for (const [name, filters] of cases) {
          const shot = (pool: InstanceType<typeof P.MotionCanvasPool>) => {
            const c = document.createElement(`canvas`)
            c.width = c.height = size
            const ctx = c.getContext(`2d`, { willReadFrequently: true })!
            P.paintMotionFrame(ctx, size, size, frame(filters), P.motionFitMatrix(200, 200, size, size), [], pool)
            return ctx.getImageData(0, 0, size, size).data
          }
          const pool = new P.MotionCanvasPool()
          const got = shot(pool)
          const want = shot(new P.MotionCanvasPool(undefined, { nativeFilters: false }))
          let matched = 0
          let maxDiff = 0
          for (let k = 0; k < got.length; k += 4) {
            let worst = Math.abs(got[k + 3]! - want[k + 3]!)
            for (let ch = 0; ch < 3; ch++) worst = Math.max(worst, Math.abs(got[k + ch]! * got[k + 3]! - want[k + ch]! * want[k + 3]!) / 255)
            worst = Math.round(worst)
            maxDiff = Math.max(maxDiff, worst)
            if (worst <= tol) matched++
          }
          out.push({ name, size, native: pool.nativeFilters(), share: matched / (got.length / 4), maxDiff })
        }
      return out
    },
    { cases, tol: MOTION_PARITY_TOLERANCE }
  )
}
