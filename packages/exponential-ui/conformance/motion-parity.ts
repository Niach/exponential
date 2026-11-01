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
