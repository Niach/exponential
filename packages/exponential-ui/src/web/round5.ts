// VAPP-162 (docs/round-5-contract.md): the framework-free halves of the
// round-5 natives every web painter (React, Solid and through it the
// element) shares, so the two renderers cannot drift: the QrCode's module
// path and its plate swap (§6), the QrScanner's distinct-value reporter
// (§7), the Segmented count badge text (§3) and THE Button hold machine
// (§9, fixtures/interactions-round1.json `buttonHold`).

import { luminance, parseColor, qrMatrix, QR_CODE, type QrMatrix } from "@exponential_app/eui"
import styleJson from "../../catalog/style.json" with { type: "json" }

/** style.json `qrScanner` (the viewfinder geometry). */
export const QR_SCANNER = styleJson.qrScanner

// ---------------------------------------------------------------------------
// QrCode (§6)
// ---------------------------------------------------------------------------

export interface QrPaint {
  /** The core's matrix (null = the empty plate). */
  matrix: QrMatrix | null
  /** The SVG viewBox edge in MODULES (n + 2 × quietZone); 0 without a matrix. */
  modules: number
  /** One path of every dark module run in module units (quiet zone
   *  included), so `shape-rendering: crispEdges` snaps the edges to device
   *  pixels while the outer size stays the exact `size`. */
  path: string
  /** The resolved colours after THE SWAP: the lighter fills the plate. */
  plate: string
  module: string
}

/** The QrCode paint of `value` at `level` (style.json `qrCode`): the core's
 *  `qrMatrix`, never a platform QR library; of the two recipe colours the
 *  lighter (relative luminance) is the plate, the darker the modules, so a
 *  dark theme still scans. A colour the parser cannot read keeps its role. */
export function qrPaint(value: string, level: unknown, plateColour: string, moduleColour: string): QrPaint {
  const matrix = qrMatrix(value, level)
  const pc = parseColor(plateColour)
  const mc = parseColor(moduleColour)
  const swap = pc !== null && mc !== null && luminance(pc) < luminance(mc)
  const plate = swap ? moduleColour : plateColour
  const module = swap ? plateColour : moduleColour
  if (!matrix) return { matrix: null, modules: 0, path: ``, plate, module }
  const q = QR_CODE.quietZone
  let path = ``
  matrix.rows.forEach((row, y) => {
    let x = 0
    while (x < row.length) {
      if (row[x] !== `1`) {
        x += 1
        continue
      }
      let end = x
      while (end < row.length && row[end] === `1`) end += 1
      path += `M${x + q} ${y + q}h${end - x}v1h${x - end}z`
      x = end
    }
  })
  return { matrix, modules: matrix.size + 2 * q, path, plate, module }
}

// ---------------------------------------------------------------------------
// QrScanner (§7): the host camera hook
// ---------------------------------------------------------------------------

/** THE web renderers' camera hook (the `qrScanner` host option; never
 *  catalog/host.json): the host renders its live feed into `feed` (an empty
 *  element filling the viewfinder; append a `<video>`), decodes codes of
 *  `formats` (default `["qr"]`) and calls `onValue` with every decoded
 *  string; the renderer dispatches `scanned {value}` once per distinct
 *  value. `start` returns the stop (called on unmount, on `disabled` and
 *  when the formats change); returning `false` (or throwing) = no camera
 *  (the placeholder shows). */
export interface QrScannerHost {
  start(feed: HTMLElement, formats: readonly string[], onValue: (value: string) => void): (() => void) | false | void
}

/** One reporter per mounted scanner: forwards a decoded value only when it
 *  differs from the last one reported (a code held in view reports once). */
export function qrScanReporter(report: (value: string) => void): (value: string) => void {
  let last: string | null = null
  return (value: string) => {
    if (typeof value !== `string` || value === `` || value === last) return
    last = value
    report(value)
  }
}

export const QR_CORNERS = [`tl`, `tr`, `bl`, `br`] as const
export type QrCorner = (typeof QR_CORNERS)[number]

/** One corner bracket as an SVG path in its own cornerLength-px square
 *  (placed flush in that corner of the viewfinder): two strokes of
 *  cornerStroke px whose outer edge follows the viewfinder radius `radius`
 *  (the stroke's centre line runs half a stroke inside). Draw it with
 *  round caps and `overflow: visible`. */
export function qrCornerPath(corner: QrCorner, radius: number): string {
  const L = QR_SCANNER.cornerLength
  const h = QR_SCANNER.cornerStroke / 2
  const r = Math.max(0, Math.min(L - h, (Number.isFinite(radius) ? radius : 0) - h))
  const fx = (x: number) => (corner === `tr` || corner === `br` ? L - x : x)
  const fy = (y: number) => (corner === `bl` || corner === `br` ? L - y : y)
  const sweep = corner === `tl` || corner === `br` ? 1 : 0
  const n = (v: number) => Math.round(v * 100) / 100
  return `M${n(fx(h))} ${n(fy(L))}V${n(fy(h + r))}A${n(r)} ${n(r)} 0 0 ${sweep} ${n(fx(h + r))} ${n(fy(h))}H${n(fx(L))}`
}

/** The empty viewfinder's fill: the `feed` recipe colour at feedAlpha. */
export function qrFeedColour(colour: string): string {
  const c = parseColor(colour)
  if (!c) return `color-mix(in srgb, ${colour} ${QR_SCANNER.feedAlpha * 100}%, transparent)`
  return `rgba(${c.r}, ${c.g}, ${c.b}, ${Math.round(c.a * QR_SCANNER.feedAlpha * 1000) / 1000})`
}

/** The formats a scanner asks the host for (style: the catalog default). */
export function qrScanFormats(raw: unknown): string[] {
  const list = Array.isArray(raw) ? raw.filter((f): f is string => typeof f === `string` && f !== ``) : []
  return list.length ? list : [`qr`]
}

// ---------------------------------------------------------------------------
// Segmented count badge (§3)
// ---------------------------------------------------------------------------

/** A `bar` item's badge text: the number, `99+` above 99; null (no badge)
 *  at 0, unset or not a number. */
export function segmentedBadge(raw: unknown): string | null {
  const n = typeof raw === `number` ? raw : typeof raw === `string` && raw.trim() !== `` ? Number(raw) : NaN
  if (!Number.isFinite(n) || n <= 0) return null
  const whole = Math.floor(n)
  if (whole <= 0) return null
  return whole > 99 ? `99+` : String(whole)
}

// ---------------------------------------------------------------------------
// Button hold (§9)
// ---------------------------------------------------------------------------

/** THE slop: how far (px) a held pointer may wander outside the box. */
export const HOLD_SLOP = 24

export type HoldEvent = `holdStart` | `holdEnd` | `holdCancel`

export interface HoldRect {
  left: number
  top: number
  right: number
  bottom: number
}

/** Whether (x, y) lies within the box grown by the slop on every side. */
export function withinSlop(rect: HoldRect, x: number, y: number, slop = HOLD_SLOP): boolean {
  return x >= rect.left - slop && x <= rect.right + slop && y >= rect.top - slop && y <= rect.bottom + slop
}

/** THE hold machine of a `hold: true` Button, framework-free. The painter
 *  forwards its DOM events; `fire` gets the catalog event, `pressed` the
 *  painted state. A press never fires `press` here (the painter skips its
 *  click while `hold`). */
export class ButtonHold {
  /** `pointer`, the key that started the hold, or null when idle. */
  private by: string | null = null
  private pointerId: number | null = null

  constructor(
    private readonly fire: (event: HoldEvent) => void,
    private readonly pressed: (pressed: boolean) => void,
    private readonly disabled: () => boolean
  ) {}

  get active(): boolean {
    return this.by !== null
  }

  private start(by: string, pointerId: number | null = null) {
    if (this.by !== null || this.disabled()) return false
    this.by = by
    this.pointerId = pointerId
    this.pressed(true)
    this.fire(`holdStart`)
    return true
  }

  private finish(event: `holdEnd` | `holdCancel`) {
    if (this.by === null) return false
    this.by = null
    this.pointerId = null
    this.pressed(false)
    this.fire(event)
    return true
  }

  /** Pointer down (primary button): starts the hold; true = claim the
   *  pointer (capture it, prevent the long-press / scroll defaults). */
  pointerDown(pointerId: number | null = null): boolean {
    return this.start(`pointer`, pointerId)
  }

  /** A move of the holding pointer: past the slop = the slide away. */
  pointerMove(rect: HoldRect, x: number, y: number, pointerId: number | null = null): void {
    if (this.by !== `pointer` || !this.samePointer(pointerId)) return
    if (!withinSlop(rect, x, y)) this.finish(`holdCancel`)
  }

  /** The holding pointer released: within the slop ends, past it cancels. */
  pointerUp(rect: HoldRect, x: number, y: number, pointerId: number | null = null): void {
    if (this.by !== `pointer` || !this.samePointer(pointerId)) return
    this.finish(withinSlop(rect, x, y) ? `holdEnd` : `holdCancel`)
  }

  pointerCancel(pointerId: number | null = null): void {
    if (this.by === `pointer` && this.samePointer(pointerId)) this.finish(`holdCancel`)
  }

  /** Key down: Space/Enter start (repeat ignored), Escape cancels. True =
   *  the key was handled (prevent its default: no click, no scroll). */
  keyDown(key: string, repeat = false): boolean {
    if (key === `Escape`) return this.finish(`holdCancel`)
    if (key !== ` ` && key !== `Enter`) return false
    if (repeat || this.by !== null) return this.by !== null
    return this.start(key)
  }

  /** Key up of the key that started the hold ends it. */
  keyUp(key: string): boolean {
    if (this.by === null || this.by !== key) return key === ` ` || key === `Enter`
    return this.finish(`holdEnd`)
  }

  blur(): void {
    this.finish(`holdCancel`)
  }

  /** Unmount or `disabled` mid-hold: drop it silently. */
  reset(): void {
    if (this.by === null) return
    this.by = null
    this.pointerId = null
    this.pressed(false)
  }

  private samePointer(id: number | null) {
    return this.pointerId === null || id === null || id === this.pointerId
  }
}
