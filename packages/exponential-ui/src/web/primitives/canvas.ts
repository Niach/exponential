// Round 8 (docs/round-8-contract.md §1): the `Canvas` native's web half,
// shared by React, Solid and <exponential-surface>. The wrappers render
// `<div role=img><canvas/></div>` (box = `canvasBoxStyle`, a11y =
// `canvasA11y`) and hand both elements to a `CanvasView`; it owns the rest,
// on Motion's machinery (the WASM engine, `motionThemeSlots`, the shared
// Canvas2D painter + its canvas pool):
//
// 1. Compile: `{size, shapes}` → the WASM `CanvasPlayer` (the constructor
//    validates: a refused drawing reports its issues as a paint error).
//    Recompiles only when `size` or `shapes` change.
// 2. Sample: ONE frame with the theme's slots (`$color.*` + `$mode`);
//    resampled only on a theme or mode change. No clock, no playback.
// 3. Paint: the frame at devicePixelRatio under the fit (`canvasFitMatrix`),
//    the `text` ops in the theme's `$type.family.sans` / `.mono`; repaints
//    on resize and once the document's fonts are loaded.
// 4. Pointer (`pointer: true` only): `press {x, y}` (a click), `move {x, y}`
//    (at most once per animation frame) and `leave`, x/y in drawing units
//    (`canvasToDrawing`: the fit inverted, two decimals, never clamped).
//
// The WASM is imported lazily on the first Canvas that paints. Without
// requestAnimationFrame or Path2D (SSR, jsdom) it paints nothing and loads
// nothing; the pointer events still fire.

// The package root is external to the web entries (like motion.ts).
import { canvasDeclaredSize, canvasFit, canvasFitMatrix, canvasToDrawing, type CanvasEvent, type ModeName, type ResolvedTheme } from "@exponential_app/eui"
import { fontStack } from "../font-stack"
import { MotionCanvasPool, MOTION_DEFAULT_FONTS, motionConcat, paintMotionFrame, type MotionTextFonts } from "./motion-paint"
import { motionThemeSlots } from "./motion"

type Props = Record<string, unknown>
type Player = import("../../motion/wasm").MotionBindings[`CanvasPlayer`][`prototype`]

export interface CanvasEnv {
  theme: ResolvedTheme
  mode: ModeName
  /** A refused drawing (the box stays empty): the wrapper reports it as a paint error. */
  onError?: (message: string) => void
  /** The node's `on` events (the wrapper's A2UI action plumbing). */
  emit?: (event: CanvasEvent, payload?: Record<string, unknown>) => void
}

/** Whether this environment paints Canvases at all. */
export function canvasCanPaint(): boolean {
  return typeof requestAnimationFrame === `function` && typeof Path2D === `function`
}

/** The font stacks a theme sets a drawing's `text` ops in. */
export function canvasThemeFonts(theme: ResolvedTheme): MotionTextFonts {
  const family = (theme.tokens?.type?.family ?? {}) as Record<string, string>
  const stack = (name: `sans` | `mono`) => (family[name] ? fontStack(family[name], theme.fonts ?? {}) : null) ?? MOTION_DEFAULT_FONTS[name]
  return { sans: stack(`sans`), mono: stack(`mono`) }
}

let themeIds = new WeakMap<object, number>()
let nextThemeId = 1
const themeId = (t: object) => {
  let id = themeIds.get(t)
  if (!id) themeIds.set(t, (id = nextThemeId++))
  return id
}

/** One Canvas on the page: compile, sample, paint, pointer. `update` on
 *  every prop or environment change; `destroy` on unmount. */
export class CanvasView {
  private props: Props = {}
  private env: CanvasEnv | null = null
  private sourceKey = ``
  private slotsKey = ``
  private generation = 0
  private player: Player | null = null
  private slotIds: string[] = []
  private frame: Float32Array | null = null
  private fonts: MotionTextFonts = MOTION_DEFAULT_FONTS
  private pool = new MotionCanvasPool()
  private ro: ResizeObserver | null = null
  private painted = ``
  private moveRaf = 0
  private moveAt: { x: number; y: number } | null = null
  private fontsWaited = false
  private destroyed = false

  constructor(
    readonly root: HTMLElement,
    readonly canvas: HTMLCanvasElement
  ) {
    root.addEventListener(`click`, this.onClick)
    root.addEventListener(`pointermove`, this.onMove)
    root.addEventListener(`pointerleave`, this.onLeave)
    if (!canvasCanPaint()) return
    if (typeof ResizeObserver === `function`) {
      this.ro = new ResizeObserver(() => this.paint())
      this.ro.observe(root)
    }
  }

  update(props: Props, env: CanvasEnv): void {
    if (this.destroyed) return
    this.props = props
    this.env = env
    if (!canvasCanPaint()) return
    const sourceKey = JSON.stringify([props.size ?? null, props.shapes ?? null])
    const slotsKey = `${themeId(env.theme)}:${env.mode}`
    if (sourceKey !== this.sourceKey) {
      this.sourceKey = sourceKey
      this.slotsKey = slotsKey
      void this.compile()
      return
    }
    if (slotsKey !== this.slotsKey) {
      this.slotsKey = slotsKey
      this.sample()
    }
    this.paint()
  }

  destroy(): void {
    this.destroyed = true
    this.generation++
    this.root.removeEventListener(`click`, this.onClick)
    this.root.removeEventListener(`pointermove`, this.onMove)
    this.root.removeEventListener(`pointerleave`, this.onLeave)
    if (this.moveRaf) cancelAnimationFrame(this.moveRaf)
    this.moveRaf = 0
    this.ro?.disconnect()
    this.player?.free()
    this.player = null
  }

  // --- pointer ---------------------------------------------------------------

  private pointerOn(): boolean {
    return this.props.pointer === true && !this.destroyed && Boolean(this.env?.emit)
  }

  /** Box px (relative to the root's top-left) → drawing units. */
  private toDrawing(clientX: number, clientY: number): { x: number; y: number } | null {
    const size = canvasDeclaredSize(this.props)
    if (!size) return null
    const rect = this.root.getBoundingClientRect()
    return canvasToDrawing(clientX - rect.left, clientY - rect.top, size[0], size[1], rect.width, rect.height, canvasFit(this.props.fit))
  }

  private onClick = (e: MouseEvent): void => {
    if (!this.pointerOn()) return
    const at = this.toDrawing(e.clientX, e.clientY)
    if (at) this.env!.emit!(`press`, at)
  }

  private onMove = (e: PointerEvent): void => {
    if (!this.pointerOn()) return
    this.moveAt = { x: e.clientX, y: e.clientY }
    if (this.moveRaf) return
    if (typeof requestAnimationFrame !== `function`) return this.flushMove()
    this.moveRaf = requestAnimationFrame(() => {
      this.moveRaf = 0
      this.flushMove()
    })
  }

  private flushMove(): void {
    const p = this.moveAt
    this.moveAt = null
    if (!p || !this.pointerOn()) return
    const at = this.toDrawing(p.x, p.y)
    if (at) this.env!.emit!(`move`, at)
  }

  private onLeave = (): void => {
    if (this.moveRaf) cancelAnimationFrame(this.moveRaf)
    this.moveRaf = 0
    this.moveAt = null
    if (this.pointerOn()) this.env!.emit!(`leave`)
  }

  // --- compile, sample, paint -----------------------------------------------

  private fail(message: string): void {
    this.root.dataset.state = `error`
    this.env?.onError?.(message)
  }

  private async compile(): Promise<void> {
    const gen = ++this.generation
    this.player?.free()
    this.player = null
    this.frame = null
    this.painted = ``
    this.root.dataset.state = `loading`
    const doc = JSON.stringify({ size: this.props.size, shapes: this.props.shapes })
    try {
      const wasm = await (await import("../../motion/wasm")).loadMotionWasm()
      if (gen !== this.generation) return
      let player: Player
      try {
        player = new wasm.CanvasPlayer(doc)
      } catch (e) {
        throw new Error(`Canvas: ${issueText(e)}`)
      }
      this.player = player
      this.slotIds = JSON.parse(player.slotIds()) as string[]
      this.root.dataset.state = `ready`
      this.sample()
      this.paint()
    } catch (e) {
      if (gen !== this.generation) return
      this.clear()
      this.fail(e instanceof Error ? e.message : String(e))
    }
  }

  private sample(): void {
    if (!this.player || !this.env) return
    this.fonts = canvasThemeFonts(this.env.theme)
    this.frame = this.player.sample(JSON.stringify(motionThemeSlots(this.slotIds, this.env.theme, this.env.mode, undefined)))
    this.painted = ``
  }

  private clear(): void {
    const ctx = this.canvas.getContext(`2d`)
    ctx?.clearRect(0, 0, this.canvas.width, this.canvas.height)
    this.painted = ``
  }

  private paint(): void {
    const frame = this.frame
    if (!frame || this.destroyed) return
    const rect = this.root.getBoundingClientRect()
    const dpr = (typeof window !== `undefined` && window.devicePixelRatio) || 1
    const w = Math.max(1, Math.round(rect.width * dpr))
    const h = Math.max(1, Math.round(rect.height * dpr))
    const fit = canvasFit(this.props.fit)
    const key = `${w}|${h}|${fit}|${this.slotsKey}`
    if (key === this.painted) return
    this.painted = key
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w
      this.canvas.height = h
    }
    const ctx = this.canvas.getContext(`2d`)
    if (!ctx) return
    const base = motionConcat([dpr, 0, 0, dpr, 0, 0], canvasFitMatrix(Number(frame[1]) || 0, Number(frame[2]) || 0, w / dpr, h / dpr, fit))
    paintMotionFrame(ctx, w, h, frame, base, [], this.pool, this.fonts)
    this.waitForFonts()
  }

  /** Text painted before the theme's web fonts arrived: paint once more. */
  private waitForFonts(): void {
    if (this.fontsWaited || typeof document === `undefined` || !document.fonts || document.fonts.status === `loaded`) return
    this.fontsWaited = true
    void document.fonts.ready.then(() => {
      this.painted = ``
      this.paint()
    })
  }
}

/** A refused drawing's issues (`CanvasPlayer` throws a JSON array). */
function issueText(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e)
  try {
    const issues = JSON.parse(raw) as { path?: string; message?: string }[]
    if (Array.isArray(issues)) return issues.map((i) => `${i.path ?? ``} ${i.message ?? ``}`.trim()).join(`; `) || `invalid drawing`
  } catch {
    // not JSON: the message as is
  }
  return raw
}

/** Test seam: forget the per-theme ids (a fresh module state). */
export function resetCanvasThemeIds(): void {
  themeIds = new WeakMap()
  nextThemeId = 1
}
