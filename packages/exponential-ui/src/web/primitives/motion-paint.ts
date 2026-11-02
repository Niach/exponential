// EXP-1604: eUI Motion's ONE web painter (React, Solid and
// <exponential-surface> all paint through it). It replays the engine's
// packed display list (apps/desktop/crates/eui-motion/README.md › Packed
// frames) on a Canvas2D context and nothing else: no Lottie, no clock.
// Semantics follow the reference raster (eui-motion `raster.rs`
// `paint_reference`): a layer is a full-size offscreen group clipped to the
// current clip ∩ its path, filtered, then composited onto its parent under
// that clip with its alpha and blend; luma mattes turn the group into a
// luminance mask first. The filters (blur, colour matrix, gradient map, drop
// shadow) are pixel ports of the raster's, so every browser paints the same
// pixels as the native painters (ctx.filter differs per engine).

export type MotionMatrix = readonly [number, number, number, number, number, number]
export type MotionFit = `contain` | `cover` | `fill`

/** A canvas a group paints into (OffscreenCanvas where there is one). */
export type MotionCanvas = HTMLCanvasElement | OffscreenCanvas
export type MotionContext = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D
export type MotionImageSource = CanvasImageSource & { width: number; height: number }

export const MOTION_PACKED_VERSION = 1

const BLENDS: readonly GlobalCompositeOperation[] = [
  `source-over`,
  `multiply`,
  `screen`,
  `overlay`,
  `darken`,
  `lighten`,
  `color-dodge`,
  `color-burn`,
  `hard-light`,
  `soft-light`,
  `difference`,
  `exclusion`,
  `hue`,
  `saturation`,
  `color`,
  `luminosity`,
  `destination-in`,
  `destination-out`,
  `destination-in`, // luma-in (after the luminance mask)
  `destination-out`, // luma-out
  `xor`,
]
const LUMA_IN = 18
const LUMA_OUT = 19
const CAPS: readonly CanvasLineCap[] = [`butt`, `round`, `square`]
const JOINS: readonly CanvasLineJoin[] = [`miter`, `round`, `bevel`]

/** The fit transform of a `cw × ch` composition into a `bw × bh` box
 *  (contain = centred, cover = centred and overflowing, fill = stretched). */
export function motionFitMatrix(cw: number, ch: number, bw: number, bh: number, fit: MotionFit = `contain`): MotionMatrix {
  if (!(cw > 0) || !(ch > 0)) return [1, 0, 0, 1, 0, 0]
  if (fit === `fill`) return [bw / cw, 0, 0, bh / ch, 0, 0]
  const s = fit === `cover` ? Math.max(bw / cw, bh / ch) : Math.min(bw / cw, bh / ch)
  return [s, 0, 0, s, (bw - cw * s) / 2, (bh - ch * s) / 2]
}

/** `a ∘ b` (b applied first). */
export function motionConcat(a: MotionMatrix, b: MotionMatrix): MotionMatrix {
  return [a[0] * b[0] + a[2] * b[1], a[1] * b[0] + a[3] * b[1], a[0] * b[2] + a[2] * b[3], a[1] * b[2] + a[3] * b[3], a[0] * b[4] + a[2] * b[5] + a[4], a[1] * b[4] + a[3] * b[5] + a[5]]
}

/** The composition size a packed frame declares. */
export function motionFrameSize(packed: ArrayLike<number>): { width: number; height: number } {
  return { width: Number(packed[1]) || 0, height: Number(packed[2]) || 0 }
}

/** Reuses group canvases across frames (one painter instance per Motion). */
export class MotionCanvasPool {
  private free: MotionCanvas[] = []
  constructor(private readonly make: (w: number, h: number) => MotionCanvas | null = defaultCanvas) {}
  take(w: number, h: number): { canvas: MotionCanvas; ctx: MotionContext } | null {
    let c = this.free.pop() ?? this.make(w, h)
    if (!c) return null
    if (c.width !== w || c.height !== h) {
      c.width = w
      c.height = h
    }
    const ctx = c.getContext(`2d`) as MotionContext | null
    if (!ctx) return null
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.globalAlpha = 1
    ctx.globalCompositeOperation = `source-over`
    ctx.clearRect(0, 0, w, h)
    return { canvas: c, ctx }
  }
  give(c: MotionCanvas): void {
    if (this.free.length < 8) this.free.push(c)
  }
}

function defaultCanvas(w: number, h: number): MotionCanvas | null {
  if (typeof OffscreenCanvas === `function`) return new OffscreenCanvas(w, h)
  if (typeof document === `undefined`) return null
  const c = document.createElement(`canvas`)
  c.width = w
  c.height = h
  return c
}

type Clip = { m: MotionMatrix; path: Path2D | null }
type Frame =
  | { kind: `clip`; clips: number }
  | { kind: `layer`; clips: number; canvas: MotionCanvas; ctx: MotionContext; blend: number; alpha: number; filters: Filter[]; clip: Clip }
type Stop = { offset: number; color: [number, number, number, number] }
type Filter = { kind: 0; sx: number; sy: number } | { kind: 1; m: number[] } | { kind: 2; amount: number; stops: Stop[] } | { kind: 3; color: [number, number, number, number]; dx: number; dy: number; sigma: number }

/** Paint one packed frame into `ctx` (a `width × height` px canvas, cleared
 *  first) under `base` (the fit transform × devicePixelRatio). `images` =
 *  the decoded assets in `MotionPlayer.images()` order (null while
 *  loading: skipped). Returns false when the stream is not a v1 frame. */
export function paintMotionFrame(ctx: MotionContext, width: number, height: number, packed: ArrayLike<number>, base: MotionMatrix, images: readonly (MotionImageSource | null | undefined)[] = [], pool: MotionCanvasPool = new MotionCanvasPool()): boolean {
  ctx.setTransform(1, 0, 0, 1, 0, 0)
  ctx.globalAlpha = 1
  ctx.globalCompositeOperation = `source-over`
  ctx.clearRect(0, 0, width, height)
  if (packed[0] !== MOTION_PACKED_VERSION) return false
  const scale = Math.sqrt(Math.abs(base[0] * base[3] - base[1] * base[2]))
  let i = 3
  const n = packed.length
  const f = () => Number(packed[i++])
  const mat = (): MotionMatrix => [f(), f(), f(), f(), f(), f()]
  const readPath = (): Path2D | null => {
    const count = f()
    const p = typeof Path2D === `function` ? new Path2D() : null
    let any = false
    for (let k = 0; k < count; k++) {
      const v = f()
      if (v === 0) {
        const x = f(), y = f()
        p?.moveTo(x, y)
        any = true
      } else if (v === 1) {
        const x = f(), y = f()
        p?.lineTo(x, y)
        any = true
      } else if (v === 2) {
        const x1 = f(), y1 = f(), x2 = f(), y2 = f(), x = f(), y = f()
        p?.bezierCurveTo(x1, y1, x2, y2, x, y)
        any = true
      } else p?.closePath()
    }
    return any ? p : null
  }
  const readStops = (): Stop[] => {
    const count = f()
    const out: Stop[] = []
    for (let k = 0; k < count; k++) out.push({ offset: f(), color: [f(), f(), f(), f()] })
    return out
  }
  type Brush = { kind: 0; color: [number, number, number, number] } | { kind: 1; x0: number; y0: number; x1: number; y1: number; stops: Stop[] } | { kind: 2; cx: number; cy: number; fx: number; fy: number; r: number; stops: Stop[] }
  const readBrush = (): Brush => {
    const kind = f()
    if (kind === 1) return { kind: 1, x0: f(), y0: f(), x1: f(), y1: f(), stops: readStops() }
    if (kind === 2) return { kind: 2, cx: f(), cy: f(), fx: f(), fy: f(), r: f(), stops: readStops() }
    return { kind: 0, color: [f(), f(), f(), f()] }
  }
  const readFilters = (): Filter[] => {
    const count = f()
    const out: Filter[] = []
    for (let k = 0; k < count; k++) {
      const kind = f()
      if (kind === 0) out.push({ kind: 0, sx: f(), sy: f() })
      else if (kind === 1) {
        const m: number[] = []
        for (let j = 0; j < 20; j++) m.push(f())
        out.push({ kind: 1, m })
      } else if (kind === 2) out.push({ kind: 2, amount: f(), stops: readStops() })
      else out.push({ kind: 3, color: [f(), f(), f(), f()], dx: f(), dy: f(), sigma: f() })
    }
    return out
  }

  const clips: Clip[] = []
  const stack: Frame[] = []
  const top = (): MotionContext => {
    for (let k = stack.length - 1; k >= 0; k--) {
      const fr = stack[k]!
      if (fr.kind === `layer`) return fr.ctx
    }
    return ctx
  }
  /** Apply the clip stack; false = nothing can show (an empty clip path). */
  const applyClips = (c: MotionContext, list: readonly Clip[]): boolean => {
    for (const clip of list) {
      if (!clip.path) return false
      const m = motionConcat(base, clip.m)
      c.setTransform(m[0], m[1], m[2], m[3], m[4], m[5])
      c.clip(clip.path, `nonzero`)
    }
    return true
  }
  const setStyle = (c: MotionContext, brush: Brush, stroke: boolean) => {
    let style: string | CanvasGradient
    if (brush.kind === 0) style = css(brush.color)
    else {
      const g = brush.kind === 1 ? c.createLinearGradient(brush.x0, brush.y0, brush.x1, brush.y1) : c.createRadialGradient(brush.fx, brush.fy, 0, brush.cx, brush.cy, Math.max(0, brush.r))
      let last = 0
      for (const s of brush.stops) {
        last = Math.max(last, clamp01(s.offset))
        g.addColorStop(last, css(s.color))
      }
      style = g
    }
    if (stroke) c.strokeStyle = style
    else c.fillStyle = style
  }

  while (i < n) {
    const op = f()
    if (op === 1) {
      const blend = f()
      const alpha = f()
      const t = mat()
      const path = readPath()
      const filters = readFilters()
      const group = pool.take(width, height)
      const clip = { m: t, path }
      if (!group) return false
      stack.push({ kind: `layer`, clips: clips.length, canvas: group.canvas, ctx: group.ctx, blend, alpha, filters, clip })
      clips.push(clip)
    } else if (op === 2) {
      const t = mat()
      const path = readPath()
      stack.push({ kind: `clip`, clips: clips.length })
      clips.push({ m: t, path })
    } else if (op === 3) {
      const fr = stack.pop()
      if (!fr) continue
      clips.length = fr.clips
      if (fr.kind === `clip`) continue
      const dst = top()
      const needsPixels = fr.blend === LUMA_IN || fr.blend === LUMA_OUT || fr.filters.length > 0
      if (needsPixels) {
        const img = fr.ctx.getImageData(0, 0, width, height)
        const px = premultiply(img.data)
        if (fr.blend === LUMA_IN || fr.blend === LUMA_OUT) lumaMask(px)
        applyFilters(px, width, height, fr.filters, base, scale)
        unpremultiply(px, img.data)
        fr.ctx.putImageData(img, 0, 0)
      }
      dst.save()
      if (applyClips(dst, [...clips, fr.clip])) {
        dst.setTransform(1, 0, 0, 1, 0, 0)
        dst.globalAlpha = clamp01(fr.alpha)
        dst.globalCompositeOperation = BLENDS[fr.blend] ?? `source-over`
        dst.drawImage(fr.canvas, 0, 0)
      }
      dst.restore()
      pool.give(fr.canvas)
    } else if (op === 4 || op === 5) {
      const t = mat()
      let rule: CanvasFillRule = `nonzero`
      let width_ = 0, cap = 0, join = 0, miter = 4, dashOffset = 0
      let dashes: number[] = []
      if (op === 4) rule = f() === 1 ? `evenodd` : `nonzero`
      else {
        width_ = f()
        cap = f()
        join = f()
        miter = f()
        dashOffset = f()
        const dn = f()
        for (let k = 0; k < dn; k++) dashes.push(f())
      }
      const brush = readBrush()
      const path = readPath()
      if (!path) continue
      const c = top()
      c.save()
      if (applyClips(c, clips)) {
        const m = motionConcat(base, t)
        c.setTransform(m[0], m[1], m[2], m[3], m[4], m[5])
        setStyle(c, brush, op === 5)
        if (op === 4) c.fill(path, rule)
        else if (width_ > 0) {
          c.lineWidth = width_
          c.lineCap = CAPS[cap] ?? `butt`
          c.lineJoin = JOINS[join] ?? `miter`
          c.miterLimit = miter > 0 ? miter : 4
          if (dashes.length && dashes.every((d) => d >= 0 && Number.isFinite(d)) && dashes.some((d) => d > 0)) {
            c.setLineDash(dashes)
            c.lineDashOffset = dashOffset
          }
          c.stroke(path)
        }
      }
      c.restore()
    } else if (op === 6) {
      const t = mat()
      const asset = f()
      const alpha = f()
      const w = f()
      const h = f()
      const img = images[asset]
      if (!img || !img.width || !img.height) continue
      const c = top()
      c.save()
      if (applyClips(c, clips)) {
        const m = motionConcat(base, t)
        c.setTransform(m[0], m[1], m[2], m[3], m[4], m[5])
        c.globalAlpha = clamp01(alpha)
        c.imageSmoothingQuality = `low`
        c.drawImage(img, 0, 0, w > 0 ? w : img.width, h > 0 ? h : img.height)
      }
      c.restore()
    } else return false
  }
  return true
}

function clamp01(v: number): number {
  return v < 0 ? 0 : v > 1 ? 1 : v
}

function css(c: readonly number[]): string {
  return `rgb(${clamp01(c[0]!) * 255} ${clamp01(c[1]!) * 255} ${clamp01(c[2]!) * 255} / ${clamp01(c[3]!)})`
}

// --- Pixel ops (ports of eui-motion raster.rs, premultiplied RGBA8) ----------

function premultiply(src: Uint8ClampedArray): Uint8Array {
  const out = new Uint8Array(src.length)
  for (let k = 0; k < src.length; k += 4) {
    const a = src[k + 3]!
    out[k + 3] = a
    if (a === 255) {
      out[k] = src[k]!
      out[k + 1] = src[k + 1]!
      out[k + 2] = src[k + 2]!
    } else if (a > 0) {
      out[k] = Math.round((src[k]! * a) / 255)
      out[k + 1] = Math.round((src[k + 1]! * a) / 255)
      out[k + 2] = Math.round((src[k + 2]! * a) / 255)
    }
  }
  return out
}

function unpremultiply(src: Uint8Array, out: Uint8ClampedArray): void {
  for (let k = 0; k < src.length; k += 4) {
    const a = src[k + 3]!
    out[k + 3] = a
    if (a === 0) {
      out[k] = out[k + 1] = out[k + 2] = 0
    } else {
      out[k] = Math.round((src[k]! * 255) / a)
      out[k + 1] = Math.round((src[k + 1]! * 255) / a)
      out[k + 2] = Math.round((src[k + 2]! * 255) / a)
    }
  }
}

function lumaMask(p: Uint8Array): void {
  for (let k = 0; k < p.length; k += 4) {
    const l = Math.max(0, Math.min(255, Math.round(0.2125 * p[k]! + 0.7154 * p[k + 1]! + 0.0721 * p[k + 2]!)))
    p[k] = p[k + 1] = p[k + 2] = p[k + 3] = l
  }
}

function applyFilters(p: Uint8Array, w: number, h: number, filters: readonly Filter[], base: MotionMatrix, scale: number): void {
  for (const fl of filters) {
    if (fl.kind === 0) blur(p, w, h, 4, fl.sx * scale, fl.sy * scale)
    else if (fl.kind === 1) colorMatrix(p, fl.m)
    else if (fl.kind === 2) gradientMap(p, fl.stops, fl.amount)
    else {
      const ddx = base[0] * fl.dx + base[2] * fl.dy
      const ddy = base[1] * fl.dx + base[3] * fl.dy
      dropShadow(p, w, h, fl.color, ddx, ddy, fl.sigma * scale)
    }
  }
}

const BOX_SIGMA = 2
const div255 = (x: number) => {
  const v = x + 128
  return (v + (v >> 8)) >> 8
}

function boxSize(sigma: number): number {
  return Math.max(0, Math.floor((sigma * 3 * Math.sqrt(2 * Math.PI)) / 4 + 0.5))
}

type Pass = { box: [number, number] } | { kernel: Uint32Array }

function passes(sigma: number): Pass[] {
  if (sigma < BOX_SIGMA) {
    const r = Math.ceil(3 * sigma)
    const ws: number[] = []
    for (let k = 0; k <= 2 * r; k++) {
      const x = k - r
      ws.push(Math.exp((-x * x) / (2 * sigma * sigma)))
    }
    const sum = ws.reduce((a, b) => a + b, 0)
    const kernel = new Uint32Array(ws.map((v) => Math.round((v / sum) * 65536)))
    const total = kernel.reduce((a, b) => a + b, 0)
    kernel[r] = Math.max(0, kernel[r]! + 65536 - total)
    return [{ kernel }]
  }
  const d = boxSize(sigma)
  if (d <= 1) return []
  const r = Math.floor(d / 2)
  return d % 2 === 1 ? [{ box: [r, r] }, { box: [r, r] }, { box: [r, r] }] : [{ box: [r, r - 1] }, { box: [r - 1, r] }, { box: [r, r] }]
}

/** Separable gaussian of a premultiplied buffer (`ch` channels), transparent outside. */
function blur(buf: Uint8Array, w: number, h: number, ch: number, sx: number, sy: number): void {
  if (!w || !h) return
  if (sx > 0 && Number.isFinite(sx)) for (const p of passes(sx)) runPass(buf, w, h, ch, p, true)
  if (sy > 0 && Number.isFinite(sy)) for (const p of passes(sy)) runPass(buf, w, h, ch, p, false)
}

/** One horizontal or vertical pass: lines along the axis, `step` apart. */
function runPass(buf: Uint8Array, w: number, h: number, ch: number, pass: Pass, horizontal: boolean): void {
  const len = horizontal ? w : h
  const lines = horizontal ? h : w
  const step = horizontal ? ch : w * ch
  const lineStep = horizontal ? w * ch : ch
  const tmp = new Uint32Array(len)
  for (let line = 0; line < lines; line++) {
    for (let c = 0; c < ch; c++) {
      const o = line * lineStep + c
      for (let x = 0; x < len; x++) tmp[x] = buf[o + x * step]!
      if (`box` in pass) {
        const [l, r] = pass.box
        const d = l + r + 1
        const half = d >> 1
        let sum = 0
        for (let k = 0; k <= Math.min(r, len - 1); k++) sum += tmp[k]!
        for (let x = 0; x < len; x++) {
          buf[o + x * step] = Math.floor((sum + half) / d)
          const add = x + r + 1
          if (add < len) sum += tmp[add]!
          if (x >= l) sum -= tmp[x - l]!
        }
      } else {
        const k = pass.kernel
        const r = k.length >> 1
        for (let x = 0; x < len; x++) {
          const lo = Math.max(0, x - r)
          const hi = Math.min(len - 1, x + r)
          let acc = 0
          for (let s = lo; s <= hi; s++) acc += k[s + r - x]! * tmp[s]!
          buf[o + x * step] = Math.min(255, Math.floor((acc + 32768) / 65536))
        }
      }
    }
  }
}

const IDENTITY_MATRIX = [1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0]

function colorMatrix(p: Uint8Array, m: readonly number[]): void {
  if (m.every((v, k) => Math.fround(v) === Math.fround(IDENTITY_MATRIX[k]!))) return
  const clearAlpha = Math.max(0, Math.min(255, Math.round(m[19]! * 255)))
  for (let k = 0; k < p.length; k += 4) {
    const a = p[k + 3]!
    if (a === 0 && clearAlpha === 0) continue
    const [r, g, b, af] = a === 0 ? [0, 0, 0, 0] : [p[k]! / a, p[k + 1]! / a, p[k + 2]! / a, a / 255]
    const row = (j: number) => clamp01(m[j]! * r + m[j + 1]! * g + m[j + 2]! * b + m[j + 3]! * af + m[j + 4]!)
    const a8 = Math.round(row(15) * 255)
    p[k] = Math.min(a8, Math.round(row(0) * a8))
    p[k + 1] = Math.min(a8, Math.round(row(5) * a8))
    p[k + 2] = Math.min(a8, Math.round(row(10) * a8))
    p[k + 3] = a8
  }
}

function gradientMap(p: Uint8Array, stops: readonly Stop[], amountRaw: number): void {
  const amount = clamp01(amountRaw)
  if (amount <= 0 || !stops.length) return
  const sorted = [...stops].sort((a, b) => a.offset - b.offset)
  const first = sorted[0]!
  const last = sorted[sorted.length - 1]!
  const lut: number[][] = []
  for (let k = 0; k < 256; k++) {
    const t = k / 255
    let c: readonly number[] = last.color
    if (t <= first.offset) c = first.color
    else if (t < last.offset) {
      for (let j = 0; j + 1 < sorted.length; j++) {
        const a = sorted[j]!, b = sorted[j + 1]!
        if (t <= b.offset) {
          const span = b.offset - a.offset
          const fr = span > 0 ? (t - a.offset) / span : 1
          c = a.color.map((v, q) => v + (b.color[q]! - v) * fr)
          break
        }
      }
    }
    lut.push([...c])
  }
  const mix = (o: number, nv: number) => clamp01(o + (clamp01(nv) - o) * amount)
  for (let k = 0; k < p.length; k += 4) {
    const a = p[k + 3]!
    if (a === 0) continue
    const r = p[k]! / a, g = p[k + 1]! / a, b = p[k + 2]! / a
    const l = clamp01(0.2126 * r + 0.7152 * g + 0.0722 * b)
    const c = lut[Math.round(l * 255)]!
    const a8 = Math.round(clamp01((a / 255) * (1 + (clamp01(c[3]!) - 1) * amount)) * 255)
    p[k] = Math.min(a8, Math.round(mix(r, c[0]!) * a8))
    p[k + 1] = Math.min(a8, Math.round(mix(g, c[1]!) * a8))
    p[k + 2] = Math.min(a8, Math.round(mix(b, c[2]!) * a8))
    p[k + 3] = a8
  }
}

/** The content over its shadow: alpha shifted by (dx, dy) device px
 *  (bilinear), blurred by `sigma`, tinted `color`. */
function dropShadow(p: Uint8Array, w: number, h: number, color: readonly number[], dxRaw: number, dyRaw: number, sigma: number): void {
  const ok = Number.isFinite(dxRaw) && Number.isFinite(dyRaw)
  const dx = ok ? dxRaw : 0
  const dy = ok ? dyRaw : 0
  const ix = Math.floor(dx), iy = Math.floor(dy)
  const wx1 = Math.round((dx - ix) * 256), wy1 = Math.round((dy - iy) * 256)
  const wx0 = 256 - wx1, wy0 = 256 - wy1
  const at = (x: number, y: number) => (x < 0 || y < 0 || x >= w || y >= h ? 0 : p[(y * w + x) * 4 + 3]!)
  const shifted = new Uint8Array(w * h)
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const sx = x - ix, sy = y - iy
      const v = (at(sx, sy) * wx0 + at(sx - 1, sy) * wx1) * wy0 + (at(sx, sy - 1) * wx0 + at(sx - 1, sy - 1) * wx1) * wy1
      shifted[y * w + x] = Math.min(255, Math.floor((v + 32768) / 65536))
    }
  }
  blur(shifted, w, h, 1, sigma, sigma)
  const ca = clamp01(color[3]!)
  const rgb = [clamp01(color[0]!), clamp01(color[1]!), clamp01(color[2]!)]
  for (let k = 0; k < w * h; k++) {
    const o = k * 4
    const sa = p[o + 3]!
    const a = shifted[k]!
    // content over shadow (premultiplied source-over)
    let sr = 0, sg = 0, sb = 0, s8 = 0
    if (a) {
      s8 = Math.min(255, Math.round(a * ca))
      sr = Math.round(rgb[0]! * s8)
      sg = Math.round(rgb[1]! * s8)
      sb = Math.round(rgb[2]! * s8)
    }
    const inv = 255 - sa
    p[o] = Math.min(255, p[o]! + div255(sr * inv))
    p[o + 1] = Math.min(255, p[o + 1]! + div255(sg * inv))
    p[o + 2] = Math.min(255, p[o + 2]! + div255(sb * inv))
    p[o + 3] = Math.min(255, sa + div255(s8 * inv))
  }
}
