// EXP-1586: the ZoomableImage native's maths (catalog/style.json
// `zoomableImage`, whose $comment is the spec). Pure: every painter feeds its
// pointer, wheel, pinch and key input through these functions, so a zoom, a
// pan or a double tap lands on the same view everywhere. The Rust core's
// `zoom` ports it; fixtures/zoom.json locks it.

import styleJson from "../catalog/style.json" with { type: "json" }

export const ZOOM = styleJson.zoomableImage

export type ZoomInitial = `auto` | `fit` | `fitWidth` | `actual`

export interface ZoomSize {
  width: number
  height: number
}

export interface ZoomPoint {
  x: number
  y: number
}

/** scale = displayed px per image px (1 = 100%); x/y = the picture's
 *  top-left in the viewport. */
export interface ZoomView {
  scale: number
  x: number
  y: number
}

/** What every function needs to know about the picture and its box. */
export interface ZoomFrame {
  natural: ZoomSize
  viewport: ZoomSize
  initial?: ZoomInitial
  maxScale?: number
}

const EPS = 1e-6
/** Results are rounded to 1e-6 so every platform prints the same numbers. */
const tidy = (v: number) => {
  const r = Math.round(v * 1e6) / 1e6
  return Object.is(r, -0) ? 0 : r
}
const tidyView = (v: ZoomView): ZoomView => ({ scale: tidy(v.scale), x: tidy(v.x), y: tidy(v.y) })
const clampN = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v))
const positive = (v: number | undefined) => (typeof v === `number` && Number.isFinite(v) && v > 0 ? v : 0)

function sizes(frame: ZoomFrame) {
  return {
    iw: positive(frame.natural.width),
    ih: positive(frame.natural.height),
    vw: positive(frame.viewport.width),
    vh: positive(frame.viewport.height),
  }
}

/** A picture taller than 1 / tallAspect times its width. */
export function zoomIsTall(natural: ZoomSize): boolean {
  const w = positive(natural.width)
  const h = positive(natural.height)
  return w > 0 && h > 0 && w / h < ZOOM.tallAspect
}

/** The whole picture inside the viewport, never upscaled: the minimum. */
export function zoomFitScale(frame: ZoomFrame): number {
  return tidy(fitRaw(frame))
}

function fitRaw(frame: ZoomFrame): number {
  const { iw, ih, vw, vh } = sizes(frame)
  if (iw === 0 || ih === 0 || vw === 0 || vh === 0) return 1
  return Math.min(1, vw / iw, vh / ih)
}

function limitsRaw(frame: ZoomFrame): { min: number; max: number } {
  const min = fitRaw(frame)
  const wanted = positive(frame.maxScale) || ZOOM.maxScale
  return { min, max: Math.max(min, wanted) }
}

/** The scale range: [fit, max(fit, maxScale)]. */
export function zoomLimits(frame: ZoomFrame): { min: number; max: number } {
  const { min, max } = limitsRaw(frame)
  return { min: tidy(min), max: tidy(max) }
}

function restRaw(frame: ZoomFrame): number {
  const { iw, vw } = sizes(frame)
  const { min, max } = limitsRaw(frame)
  const initial = frame.initial ?? `auto`
  const mode = initial === `auto` ? (zoomIsTall(frame.natural) ? `fitWidth` : `fit`) : initial
  let scale = min
  if (mode === `fitWidth`) scale = iw > 0 && vw > 0 ? Math.min(1, vw / iw) : 1
  else if (mode === `actual`) scale = 1
  return clampN(scale, min, max)
}

/** The scale the picture opens at (and a double tap / `0` / the fit button
 *  returns to), inside the limits. */
export function zoomRestScale(frame: ZoomFrame): number {
  return tidy(restRaw(frame))
}

/** Clamp one axis: centred when the scaled picture fits, else no gap. */
function clampAxis(offset: number, scaled: number, extent: number): number {
  if (scaled <= extent + EPS) return (extent - scaled) / 2
  return clampN(offset, extent - scaled, 0)
}

/** The view inside its bounds (scale within the limits, no letterbox). */
export function zoomClamp(view: ZoomView, frame: ZoomFrame): ZoomView {
  const { iw, ih, vw, vh } = sizes(frame)
  const { min, max } = limitsRaw(frame)
  const scale = clampN(view.scale, min, max)
  return tidyView({ scale, x: clampAxis(view.x, iw * scale, vw), y: clampAxis(view.y, ih * scale, vh) })
}

/** The opening view: the rest scale, centred; a taller-than-the-viewport
 *  picture starts at its top. */
export function zoomRestView(frame: ZoomFrame): ZoomView {
  const { iw, vw } = sizes(frame)
  const scale = restRaw(frame)
  return zoomClamp({ scale, x: (vw - iw * scale) / 2, y: 0 }, frame)
}

/** Whether the view sits at its rest scale. */
export function zoomAtRest(view: ZoomView, frame: ZoomFrame): boolean {
  return Math.abs(view.scale - restRaw(frame)) < 1e-4
}

/** Zoom to `scale` (clamped) keeping the image point under `anchor` fixed. */
export function zoomTo(view: ZoomView, scale: number, anchor: ZoomPoint, frame: ZoomFrame): ZoomView {
  const { min, max } = limitsRaw(frame)
  const next = clampN(scale, min, max)
  const ratio = view.scale > 0 ? next / view.scale : 1
  return zoomClamp(
    { scale: next, x: anchor.x - (anchor.x - view.x) * ratio, y: anchor.y - (anchor.y - view.y) * ratio },
    frame
  )
}

/** The viewport's centre, the anchor of keys and the pill. */
export function zoomCentre(frame: ZoomFrame): ZoomPoint {
  const { vw, vh } = sizes(frame)
  return { x: vw / 2, y: vh / 2 }
}

export type WheelDeltaMode = `pixel` | `line`

/** ⌘/ctrl + wheel (or a trackpad pinch delivered as a wheel): the scale
 *  factor of one wheel event. */
export function zoomWheelFactor(deltaY: number, mode: WheelDeltaMode = `pixel`): number {
  const px = mode === `line` ? deltaY * ZOOM.wheelLinePx : deltaY
  return Math.exp(-px * ZOOM.wheelRate)
}

/** One ⌘/ctrl-wheel event about the cursor. */
export function zoomWheel(view: ZoomView, deltaY: number, mode: WheelDeltaMode, anchor: ZoomPoint, frame: ZoomFrame): ZoomView {
  return zoomTo(view, view.scale * zoomWheelFactor(deltaY, mode), anchor, frame)
}

/** A plain wheel / two-finger scroll: pans by the deltas, clamped. */
export function zoomScroll(view: ZoomView, deltaX: number, deltaY: number, frame: ZoomFrame): ZoomView {
  return zoomClamp({ scale: view.scale, x: view.x - deltaX, y: view.y - deltaY }, frame)
}

/** The ladder steps walk: the presets plus the rest scale, inside the
 *  limits, ascending. */
export function zoomLadder(frame: ZoomFrame): number[] {
  const { min, max } = limitsRaw(frame)
  const rest = restRaw(frame)
  const all = [...ZOOM.presets, min, rest, max].filter((s) => s >= min - EPS && s <= max + EPS)
  const out: number[] = []
  for (const s of all.sort((a, b) => a - b)) if (out.length === 0 || s - out[out.length - 1] > 1e-4) out.push(tidy(s))
  return out
}

/** The next ladder scale strictly above (`1`) or below (`-1`) the current
 *  one, or the current one at the end. */
export function zoomStepScale(scale: number, direction: 1 | -1, frame: ZoomFrame): number {
  const ladder = zoomLadder(frame)
  if (direction > 0) return ladder.find((s) => s > scale + 1e-4) ?? ladder[ladder.length - 1]
  for (let i = ladder.length - 1; i >= 0; i--) if (ladder[i] < scale - 1e-4) return ladder[i]
  return ladder[0]
}

/** + / − (keys and the pill) about the viewport centre. */
export function zoomStep(view: ZoomView, direction: 1 | -1, frame: ZoomFrame): ZoomView {
  return zoomTo(view, zoomStepScale(view.scale, direction, frame), zoomCentre(frame), frame)
}

/** The scale a double tap at rest jumps to. */
export function zoomDoubleTapScale(frame: ZoomFrame): number {
  const { max } = limitsRaw(frame)
  return tidy(Math.min(max, Math.max(1, ZOOM.doubleTapFactor * restRaw(frame))))
}

/** Double click / tap: at rest → zoom in about the point; else → rest. */
export function zoomDoubleTap(view: ZoomView, point: ZoomPoint, frame: ZoomFrame): ZoomView {
  if (zoomAtRest(view, frame)) return zoomTo(view, zoomDoubleTapScale(frame), point, frame)
  return zoomRestView(frame)
}

export type ZoomKey = `+` | `=` | `-` | `−` | `0` | `1`

/** A key while the picture has focus; null = not a zoom key. */
export function zoomKey(view: ZoomView, key: string, frame: ZoomFrame): ZoomView | null {
  if (key === `+` || key === `=`) return zoomStep(view, 1, frame)
  if (key === `-` || key === `−` || key === `_`) return zoomStep(view, -1, frame)
  if (key === `0`) return zoomRestView(frame)
  if (key === `1`) return zoomTo(view, 1, zoomCentre(frame), frame)
  return null
}

/** The overshoot past an edge, damped: (1 − 1 / (o·c/d + 1))·d. */
export function zoomRubberBand(overshoot: number, extent: number): number {
  if (extent <= 0 || overshoot === 0) return 0
  const sign = overshoot < 0 ? -1 : 1
  const o = Math.abs(overshoot)
  return tidy(sign * (1 - 1 / ((o * ZOOM.rubber) / extent + 1)) * extent)
}

function rubberAxis(offset: number, scaled: number, extent: number): number {
  const lo = scaled <= extent + EPS ? (extent - scaled) / 2 : extent - scaled
  const hi = scaled <= extent + EPS ? (extent - scaled) / 2 : 0
  if (offset > hi) return hi + zoomRubberBand(offset - hi, extent)
  if (offset < lo) return lo + zoomRubberBand(offset - lo, extent)
  return offset
}

/** A drag from `start` by (dx, dy) while held: past an edge the overshoot
 *  rubber-bands. The release is `zoomClamp` of the last view. */
export function zoomDrag(start: ZoomView, dx: number, dy: number, frame: ZoomFrame): ZoomView {
  const { iw, ih, vw, vh } = sizes(frame)
  return tidyView({
    scale: start.scale,
    x: rubberAxis(start.x + dx, iw * start.scale, vw),
    y: rubberAxis(start.y + dy, ih * start.scale, vh),
  })
}

/** A pinch from `start` by the raw factor about `anchor` while held: past
 *  the min/max the scale rubber-bands (d = the limit). The release is
 *  `zoomClamp` of the last view. */
export function zoomPinch(start: ZoomView, factor: number, anchor: ZoomPoint, frame: ZoomFrame): ZoomView {
  const { iw, ih, vw, vh } = sizes(frame)
  const { min, max } = limitsRaw(frame)
  const raw = start.scale * factor
  let scale = raw
  if (raw > max) scale = max + zoomRubberBand(raw - max, max)
  else if (raw < min) scale = min + zoomRubberBand(raw - min, min)
  const ratio = start.scale > 0 ? scale / start.scale : 1
  return tidyView({
    scale,
    x: rubberAxis(anchor.x - (anchor.x - start.x) * ratio, iw * scale, vw),
    y: rubberAxis(anchor.y - (anchor.y - start.y) * ratio, ih * scale, vh),
  })
}

/** Mouse-drag momentum `t` ms after a release at velocity (px/ms),
 *  exponential decay, stopping at the edges. */
export function zoomMomentum(view: ZoomView, vx: number, vy: number, t: number, frame: ZoomFrame): ZoomView {
  const tau = ZOOM.momentumTauMs
  const travel = tau * (1 - Math.exp(-Math.max(0, t) / tau))
  return zoomClamp({ scale: view.scale, x: view.x + vx * travel, y: view.y + vy * travel }, frame)
}

/** Whether momentum `t` ms after a release is still moving (above 0.01 px/ms). */
export function zoomMomentumActive(vx: number, vy: number, t: number): boolean {
  const decay = Math.exp(-Math.max(0, t) / ZOOM.momentumTauMs)
  return Math.hypot(vx, vy) * decay > 0.01
}

/** The axes the picture can pan on: an axis without room hands a swipe to
 *  the host (paging). */
export function zoomPanRoom(view: ZoomView, frame: ZoomFrame): { x: boolean; y: boolean } {
  const { iw, ih, vw, vh } = sizes(frame)
  return { x: iw * view.scale > vw + EPS, y: ih * view.scale > vh + EPS }
}

/** A new viewport: at rest = the new rest view; zoomed = the same scale
 *  about the viewport centre, clamped. */
export function zoomResize(view: ZoomView, from: ZoomSize, frame: ZoomFrame): ZoomView {
  const before: ZoomFrame = { ...frame, viewport: from }
  if (zoomAtRest(view, before)) return zoomRestView(frame)
  const { vw, vh } = sizes(frame)
  const cx = positive(from.width) / 2
  const cy = positive(from.height) / 2
  return zoomClamp({ scale: view.scale, x: vw / 2 - (cx - view.x), y: vh / 2 - (cy - view.y) }, frame)
}

/** The pill's label: round(scale × 100) + `%`. */
export function zoomPercent(scale: number): string {
  return `${Math.round(scale * 100)}%`
}

export interface ZoomMenuItem {
  /** null = Fit (the rest view). */
  scale: number | null
  label: string
  checked: boolean
}

/** The % button's Menu: Fit, then the menu presets inside the limits. */
export function zoomMenu(view: ZoomView, frame: ZoomFrame, fitLabel: string): ZoomMenuItem[] {
  const { min, max } = limitsRaw(frame)
  const atRest = zoomAtRest(view, frame)
  const items: ZoomMenuItem[] = []
  for (const s of ZOOM.menuPresets) {
    if (s === 0) items.push({ scale: null, label: fitLabel, checked: atRest })
    else if (s >= min - EPS && s <= max + EPS) items.push({ scale: s, label: zoomPercent(s), checked: !atRest && Math.abs(view.scale - s) < 1e-4 })
  }
  return items
}

/** The pill's disabled ends. */
export function zoomBounds(view: ZoomView, frame: ZoomFrame): { atMin: boolean; atMax: boolean } {
  const { min, max } = limitsRaw(frame)
  return { atMin: view.scale <= min + 1e-4, atMax: view.scale >= max - 1e-4 }
}
