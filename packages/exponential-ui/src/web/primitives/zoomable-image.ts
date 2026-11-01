// EXP-1586: THE ZoomableImage of both web painters (the React and Solid
// natives and their primitive twins `ZoomableImage`, the same markup): a
// picture that zooms and pans inside its box, with THE zoom pill. The spec is
// catalog/style.json `zoomableImage` ($comment); the maths is the core's
// src/zoom.ts (fixtures/zoom.json), INJECTED as `ZoomMath` (the painters pass
// the functions they import from `@exponential_app/eui`), so this barrel
// carries neither style.json nor a second copy of a formula. This module holds
// what the four bindings share: the gesture controller (pointer, wheel,
// Safari's gesture events, keys, resize, the settle spring, mouse momentum,
// the pill's auto-hide), the box's aspect rule, the native's layout styles
// (the theme recipe styles the parts; these only place them) and the
// primitive's class recipes.

import type * as Zoom from "../../zoom"
import type { ZoomFrame, ZoomInitial, ZoomPoint, ZoomSize, ZoomView } from "../../zoom"


type Core = typeof Zoom

/** The core functions the controller drives (`@exponential_app/eui`). */
export type ZoomMath = Pick<
  Core,
  | `ZOOM`
  | `zoomIsTall`
  | `zoomRestView`
  | `zoomClamp`
  | `zoomAtRest`
  | `zoomTo`
  | `zoomCentre`
  | `zoomWheel`
  | `zoomScroll`
  | `zoomStep`
  | `zoomDoubleTap`
  | `zoomKey`
  | `zoomDrag`
  | `zoomPinch`
  | `zoomMomentum`
  | `zoomMomentumActive`
  | `zoomPanRoom`
  | `zoomResize`
  | `zoomPercent`
  | `zoomMenu`
  | `zoomBounds`
>

export const ZOOM_CONTROLS = [`auto`, `always`, `none`] as const
export type ZoomControls = (typeof ZOOM_CONTROLS)[number]

/** What the pill draws (changes only when one of these does). */
export interface ZoomPillState {
  percent: string
  atMin: boolean
  atMax: boolean
  atRest: boolean
}

/** Two taps / clicks closer than this (ms, px) are a double tap. */
export const ZOOM_DOUBLE_TAP_MS = 300
export const ZOOM_DOUBLE_TAP_SLOP = 24
/** A press that moves less than this (px) is a tap, not a drag. */
export const ZOOM_TAP_SLOP = 4
/** A ⌘/ctrl-wheel run settles (fires `zoom`) after this idle (ms). */
export const ZOOM_WHEEL_SETTLE_MS = 150

/** The box's CSS aspect ratio (w/h): the picture's own, capped at the width
 *  for a TALL picture (a square window onto its top), else 16:9; none when
 *  the style sets a height. */
export function zoomBoxAspect(math: Pick<ZoomMath, `zoomIsTall`>, natural: ZoomSize | null, styledHeight: boolean): number | null {
  if (styledHeight) return null
  if (!natural || !(natural.width > 0) || !(natural.height > 0)) return 16 / 9
  if (math.zoomIsTall(natural)) return 1
  return natural.width / natural.height
}

/** The natural size the props declare (both positive), else null. */
export function zoomDeclaredSize(width: unknown, height: unknown): ZoomSize | null {
  const w = typeof width === `number` && Number.isFinite(width) ? width : 0
  const h = typeof height === `number` && Number.isFinite(height) ? height : 0
  return w > 0 && h > 0 ? { width: w, height: h } : null
}

/** The menu's items (Fit, then the presets inside the limits) for the
 *  current view, before the controller measured (the rest view's). */
export function zoomPillOf(math: ZoomMath, view: ZoomView, frame: ZoomFrame): ZoomPillState {
  const { atMin, atMax } = math.zoomBounds(view, frame)
  return { percent: math.zoomPercent(view.scale), atMin, atMax, atRest: math.zoomAtRest(view, frame) }
}

export interface ZoomControllerOptions {
  math: ZoomMath
  /** The viewport: focusable, takes every gesture. */
  root: HTMLElement
  /** The picture, transformed (`transform-origin: 0 0`). */
  image: HTMLElement
  /** The zoom pill (its hover / focus keep it shown); null = none. */
  pill: () => HTMLElement | null
  /** The picture's pixel size, null while unknown. */
  natural: () => ZoomSize | null
  initial: () => ZoomInitial
  maxScale: () => number | undefined
  /** The scale the host binds (`scale`), undefined = the rest view. */
  boundScale: () => number | undefined
  /** The CSS `transition` of an animated change (the theme's standard
   *  motion); null = snap (reduced motion). */
  transition: () => string | null
  /** `controls: auto` (the pill hides on its own). */
  autoHide: () => boolean
  onPill: (pill: ZoomPillState) => void
  onPillVisible: (visible: boolean) => void
  /** A settled zoom change. */
  onZoom: (scale: number, atFit: boolean) => void
  /** A single tap / click at rest. */
  onTap: () => void
}

export interface ZoomController {
  view: () => ZoomView
  frame: () => ZoomFrame | null
  /** The pill's − / +. */
  step: (direction: 1 | -1) => void
  /** Back to the rest view (the fit button, the menu's Fit). */
  rest: () => void
  /** To `scale` about the viewport centre (the menu's presets). */
  zoomScale: (scale: number) => void
  /** The bound `scale` changed from outside: apply it, no event. */
  syncScale: () => void
  /** A new picture / natural size / initial / max: the rest view, at once. */
  reset: () => void
  /** Keep the pill shown (its menu is open). */
  holdPill: (hold: boolean) => void
  destroy: () => void
}

interface Sample {
  t: number
  x: number
  y: number
}

const sameView = (a: ZoomView, b: ZoomView) => Math.abs(a.scale - b.scale) < 1e-6 && Math.abs(a.x - b.x) < 1e-3 && Math.abs(a.y - b.y) < 1e-3
const samePill = (a: ZoomPillState | null, b: ZoomPillState) => !!a && a.percent === b.percent && a.atMin === b.atMin && a.atMax === b.atMax && a.atRest === b.atRest
const now = () => (typeof performance !== `undefined` ? performance.now() : Date.now())
const raf = (fn: (t: number) => void): number => (typeof requestAnimationFrame === `function` ? requestAnimationFrame(fn) : (setTimeout(() => fn(now()), 16) as unknown as number))
const cancelRaf = (id: number) => (typeof cancelAnimationFrame === `function` ? cancelAnimationFrame(id) : clearTimeout(id))

/** Binds the gestures to `root` and paints `image`; the pill's state and
 *  visibility go out through the callbacks. Every number comes from the
 *  core's functions. */
export function createZoomController(o: ZoomControllerOptions): ZoomController {
  const m = o.math
  const { root, image } = o
  let view: ZoomView = { scale: 1, x: 0, y: 0 }
  let viewport: ZoomSize = { width: 0, height: 0 }
  let painted = false
  let pill: ZoomPillState | null = null
  let lastEmitted: number | null = null
  // Gestures.
  const pointers = new Map<number, { x: number; y: number; type: string }>()
  let drag: { id: number; type: string; start: ZoomView; sx: number; sy: number; moved: boolean; samples: Sample[] } | null = null
  let pinch: { start: ZoomView; dist: number; mid: ZoomPoint } | null = null
  let safari: { start: ZoomView; anchor: ZoomPoint } | null = null
  let momentum = 0
  let wheelTimer: ReturnType<typeof setTimeout> | null = null
  let tapTimer: ReturnType<typeof setTimeout> | null = null
  let lastTap: { t: number; x: number; y: number } | null = null
  // The pill.
  let visible = true
  let hideTimer: ReturnType<typeof setTimeout> | null = null
  let hovered = false
  let held = false

  const frame = (): ZoomFrame | null => {
    const natural = o.natural()
    if (!natural || viewport.width <= 0 || viewport.height <= 0) return null
    return { natural, viewport, initial: o.initial(), maxScale: o.maxScale() }
  }
  const point = (e: { clientX: number; clientY: number }): ZoomPoint => {
    const r = root.getBoundingClientRect()
    return { x: e.clientX - r.left, y: e.clientY - r.top }
  }

  const cursor = () => {
    const f = frame()
    if (!f) return
    root.style.cursor = drag && drag.type === `mouse` && drag.moved ? `grabbing` : m.zoomAtRest(view, f) ? `zoom-in` : `grab`
    const room = m.zoomPanRoom(view, f)
    // A swipe along an axis without room belongs to the host (paging,
    // scrolling the page); a pinch is always ours.
    root.style.touchAction = room.x && room.y ? `none` : room.x ? `pan-y` : room.y ? `pan-x` : `pan-x pan-y`
  }

  const paint = (next: ZoomView, animated = false) => {
    view = next
    const transition = animated ? o.transition() : null
    image.style.transition = transition ?? ``
    image.style.transform = `translate(${next.x}px, ${next.y}px) scale(${next.scale})`
    if (!painted) {
      painted = true
      image.style.visibility = ``
    }
    const f = frame()
    if (f) {
      const p = zoomPillOf(m, next, f)
      if (!samePill(pill, p)) {
        pill = p
        o.onPill(p)
      }
    }
    cursor()
  }

  const settle = () => {
    const f = frame()
    if (!f) return
    if (lastEmitted !== null && Math.abs(lastEmitted - view.scale) < 1e-6) return
    lastEmitted = view.scale
    o.onZoom(view.scale, m.zoomAtRest(view, f))
  }

  const stopMotion = () => {
    if (momentum) cancelRaf(momentum)
    momentum = 0
  }

  // ── the pill's auto-hide ────────────────────────────────────────────
  const setVisible = (next: boolean) => {
    if (visible === next) return
    visible = next
    o.onPillVisible(next)
  }
  const kept = () => {
    const el = o.pill()
    return held || hovered || (!!el && typeof document !== `undefined` && el.contains(document.activeElement))
  }
  const armHide = () => {
    if (hideTimer) clearTimeout(hideTimer)
    hideTimer = null
    if (!o.autoHide()) return
    hideTimer = setTimeout(() => {
      hideTimer = null
      if (!kept()) setVisible(false)
    }, m.ZOOM.autoHideMs)
  }
  const activity = () => {
    if (!o.autoHide()) return
    setVisible(true)
    armHide()
  }

  /** A discrete zoom (keys, the pill, a double tap): animated, settled. */
  const jump = (next: ZoomView) => {
    stopMotion()
    paint(next, true)
    settle()
    activity()
  }

  const restView = (f: ZoomFrame): ZoomView => {
    const rest = m.zoomRestView(f)
    const bound = o.boundScale()
    return bound !== undefined && Number.isFinite(bound) && bound > 0 ? m.zoomTo(rest, bound, m.zoomCentre(f), f) : rest
  }

  const reset = () => {
    stopMotion()
    drag = null
    pinch = null
    const f = frame()
    if (!f) {
      painted = false
      image.style.visibility = `hidden`
      return
    }
    paint(restView(f))
    lastEmitted = view.scale
  }

  const measure = () => {
    const next = { width: root.clientWidth, height: root.clientHeight }
    if (next.width === viewport.width && next.height === viewport.height) return
    const from = viewport
    const before = frame()
    viewport = next
    const f = frame()
    if (!f) return
    if (!before || !painted) {
      paint(restView(f))
      lastEmitted = view.scale
      return
    }
    paint(m.zoomResize(view, from, f))
  }

  // ── a single tap / click and the double tap ─────────────────────────
  const tap = (at: ZoomPoint, type: string) => {
    const t = now()
    if (lastTap && t - lastTap.t < ZOOM_DOUBLE_TAP_MS && Math.hypot(at.x - lastTap.x, at.y - lastTap.y) < ZOOM_DOUBLE_TAP_SLOP) {
      if (tapTimer) clearTimeout(tapTimer)
      tapTimer = null
      lastTap = null
      const f = frame()
      if (f) jump(m.zoomDoubleTap(view, at, f))
      return
    }
    lastTap = { t, x: at.x, y: at.y }
    if (tapTimer) clearTimeout(tapTimer)
    tapTimer = setTimeout(() => {
      tapTimer = null
      lastTap = null
      if (type === `touch` && o.autoHide()) {
        if (visible) setVisible(false)
        else activity()
      }
      const f = frame()
      if (f && m.zoomAtRest(view, f)) o.onTap()
    }, ZOOM_DOUBLE_TAP_MS)
  }

  /** A released drag / pinch: the clamped view, sprung; a mouse drag
   *  inside the bounds coasts on its momentum. */
  const release = (vx = 0, vy = 0) => {
    const f = frame()
    if (!f) return
    const clamped = m.zoomClamp(view, f)
    if (!sameView(clamped, view)) {
      paint(clamped, true)
      settle()
      return
    }
    settle()
    if (!m.zoomMomentumActive(vx, vy, 0)) return
    const from = view
    const started = now()
    const step = () => {
      const t = now() - started
      const g = frame()
      if (!g) return
      paint(m.zoomMomentum(from, vx, vy, t, g))
      momentum = m.zoomMomentumActive(vx, vy, t) ? raf(step) : 0
    }
    momentum = raf(step)
  }

  const velocity = (samples: Sample[]): { vx: number; vy: number } => {
    const last = samples[samples.length - 1]
    if (!last || now() - last.t > 50) return { vx: 0, vy: 0 }
    const first = samples.find((s) => last.t - s.t <= 100) ?? last
    const dt = last.t - first.t
    return dt > 0 ? { vx: (last.x - first.x) / dt, vy: (last.y - first.y) / dt } : { vx: 0, vy: 0 }
  }

  const inPill = (target: EventTarget | null) => {
    const el = o.pill()
    return !!el && target instanceof Node && el.contains(target)
  }

  const startPinch = () => {
    const [a, b] = [...pointers.values()]
    if (!a || !b) return
    drag = null
    pinch = { start: view, dist: Math.max(1, Math.hypot(a.x - b.x, a.y - b.y)), mid: { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 } }
  }

  const onPointerDown = (e: PointerEvent) => {
    if (inPill(e.target)) return
    if (e.pointerType === `mouse` && e.button !== 0) return
    const f = frame()
    if (!f) return
    stopMotion()
    // A spring in flight stops where the person caught it.
    if (image.style.transition) paint(view)
    const at = point(e)
    pointers.set(e.pointerId, { x: at.x, y: at.y, type: e.pointerType })
    try {
      root.setPointerCapture(e.pointerId)
    } catch {
      // A synthetic pointer (tests) cannot be captured.
    }
    if (pointers.size === 2 && e.pointerType === `touch`) {
      startPinch()
      return
    }
    if (pointers.size === 1) drag = { id: e.pointerId, type: e.pointerType, start: view, sx: at.x, sy: at.y, moved: false, samples: [{ t: now(), x: at.x, y: at.y }] }
  }

  const onPointerMove = (e: PointerEvent) => {
    if (e.pointerType === `mouse` || pointers.has(e.pointerId)) activity()
    const p = pointers.get(e.pointerId)
    if (!p) return
    const at = point(e)
    p.x = at.x
    p.y = at.y
    const f = frame()
    if (!f) return
    if (pinch) {
      const [a, b] = [...pointers.values()]
      if (!a || !b) return
      const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }
      const dist = Math.max(1, Math.hypot(a.x - b.x, a.y - b.y))
      // The fingers' midpoint carries the picture as it scales about it.
      const start = { scale: pinch.start.scale, x: pinch.start.x + mid.x - pinch.mid.x, y: pinch.start.y + mid.y - pinch.mid.y }
      paint(m.zoomPinch(start, dist / pinch.dist, mid, f))
      e.preventDefault()
      return
    }
    if (!drag || drag.id !== e.pointerId) return
    let dx = at.x - drag.sx
    let dy = at.y - drag.sy
    if (!drag.moved && Math.hypot(dx, dy) < ZOOM_TAP_SLOP) return
    if (drag.type === `touch`) {
      // One finger pans only where there is room; the rest is the host's.
      const room = m.zoomPanRoom(drag.start, f)
      if (!room.x) dx = 0
      if (!room.y) dy = 0
      if (!drag.moved && dx === 0 && dy === 0) return
    }
    drag.moved = true
    drag.samples.push({ t: now(), x: at.x, y: at.y })
    if (drag.samples.length > 8) drag.samples.shift()
    paint(m.zoomDrag(drag.start, dx, dy, f))
    e.preventDefault()
  }

  const onPointerUp = (e: PointerEvent) => {
    const had = pointers.delete(e.pointerId)
    if (!had) return
    const cancelled = e.type === `pointercancel`
    if (pinch) {
      if (pointers.size < 2) {
        pinch = null
        drag = null
        release()
      }
      return
    }
    const d = drag
    if (!d || d.id !== e.pointerId) return
    drag = null
    if (!d.moved) {
      cursor()
      if (!cancelled) tap({ x: d.sx, y: d.sy }, d.type)
      return
    }
    const v = d.type === `mouse` && !cancelled ? velocity(d.samples) : { vx: 0, vy: 0 }
    release(v.vx, v.vy)
  }

  const onWheel = (e: WheelEvent) => {
    const f = frame()
    if (!f) return
    const mode = e.deltaMode === 1 ? `line` : `pixel`
    if (e.ctrlKey || e.metaKey) {
      // ⌘/ctrl + wheel, or a trackpad pinch (Chrome, Firefox): zoom about
      // the cursor; never the page's zoom.
      e.preventDefault()
      stopMotion()
      const page = e.deltaMode === 2 ? viewport.height : 1
      paint(m.zoomWheel(view, e.deltaY * page, mode, point(e), f))
      activity()
      if (wheelTimer) clearTimeout(wheelTimer)
      wheelTimer = setTimeout(() => {
        wheelTimer = null
        settle()
      }, ZOOM_WHEEL_SETTLE_MS)
      return
    }
    // A plain wheel / two-finger scroll pans; at an edge (or without room)
    // the page scrolls on.
    const unit = e.deltaMode === 1 ? m.ZOOM.wheelLinePx : e.deltaMode === 2 ? viewport.height : 1
    const next = m.zoomScroll(view, e.deltaX * unit, e.deltaY * unit, f)
    if (sameView(next, view)) return
    e.preventDefault()
    stopMotion()
    paint(next)
    activity()
  }

  // Safari's trackpad pinch (GestureEvent; Chrome and Firefox send ⌘-wheels).
  const onGestureStart = (e: Event) => {
    e.preventDefault()
    const g = e as Event & { clientX: number; clientY: number }
    if (!frame()) return
    stopMotion()
    safari = { start: view, anchor: point(g) }
  }
  const onGestureChange = (e: Event) => {
    e.preventDefault()
    const f = frame()
    if (!safari || !f) return
    paint(m.zoomPinch(safari.start, (e as Event & { scale: number }).scale, safari.anchor, f))
    activity()
  }
  const onGestureEnd = (e: Event) => {
    e.preventDefault()
    if (!safari) return
    safari = null
    release()
  }

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey || e.defaultPrevented) return
    if (inPill(e.target) && (e.key === `Enter` || e.key === ` `)) return
    const f = frame()
    if (!f) return
    const next = m.zoomKey(view, e.key, f)
    if (!next) return
    e.preventDefault()
    jump(next)
  }

  const onFocusIn = () => activity()
  const onFocusOut = () => armHide()
  // The pill is hovered while the pointer is over it (delegated: the pill
  // may mount and unmount under the controller).
  const onPointerOver = (e: PointerEvent) => {
    hovered = inPill(e.target)
  }
  const onPointerOut = (e: PointerEvent) => {
    if (!hovered || inPill(e.relatedTarget)) return
    hovered = false
    armHide()
  }
  const onDragStart = (e: Event) => e.preventDefault()

  root.addEventListener(`pointerdown`, onPointerDown)
  root.addEventListener(`pointermove`, onPointerMove)
  root.addEventListener(`pointerup`, onPointerUp)
  root.addEventListener(`pointercancel`, onPointerUp)
  root.addEventListener(`wheel`, onWheel, { passive: false })
  root.addEventListener(`gesturestart`, onGestureStart)
  root.addEventListener(`gesturechange`, onGestureChange)
  root.addEventListener(`gestureend`, onGestureEnd)
  root.addEventListener(`keydown`, onKeyDown)
  root.addEventListener(`focusin`, onFocusIn)
  root.addEventListener(`focusout`, onFocusOut)
  root.addEventListener(`dragstart`, onDragStart)
  root.addEventListener(`pointerover`, onPointerOver)
  root.addEventListener(`pointerout`, onPointerOut)

  let ro: ResizeObserver | null = null
  if (typeof ResizeObserver === `function`) {
    ro = new ResizeObserver(() => measure())
    ro.observe(root)
  }
  if (!frame()) image.style.visibility = `hidden`
  measure()
  armHide()

  return {
    view: () => view,
    frame,
    step: (direction) => {
      const f = frame()
      if (f) jump(m.zoomStep(view, direction, f))
    },
    rest: () => {
      const f = frame()
      if (f) jump(m.zoomRestView(f))
    },
    zoomScale: (scale) => {
      const f = frame()
      if (f) jump(m.zoomTo(view, scale, m.zoomCentre(f), f))
    },
    syncScale: () => {
      const f = frame()
      const bound = o.boundScale()
      if (!f || bound === undefined || !Number.isFinite(bound) || Math.abs(bound - view.scale) < 1e-4) return
      stopMotion()
      paint(m.zoomTo(view, bound, m.zoomCentre(f), f), true)
      lastEmitted = view.scale
    },
    reset: () => {
      measure()
      reset()
    },
    holdPill: (hold) => {
      held = hold
      if (hold) setVisible(true)
      else armHide()
    },
    destroy: () => {
      stopMotion()
      if (wheelTimer) clearTimeout(wheelTimer)
      if (tapTimer) clearTimeout(tapTimer)
      if (hideTimer) clearTimeout(hideTimer)
      ro?.disconnect()
      root.removeEventListener(`pointerdown`, onPointerDown)
      root.removeEventListener(`pointermove`, onPointerMove)
      root.removeEventListener(`pointerup`, onPointerUp)
      root.removeEventListener(`pointercancel`, onPointerUp)
      root.removeEventListener(`wheel`, onWheel)
      root.removeEventListener(`gesturestart`, onGestureStart)
      root.removeEventListener(`gesturechange`, onGestureChange)
      root.removeEventListener(`gestureend`, onGestureEnd)
      root.removeEventListener(`keydown`, onKeyDown)
      root.removeEventListener(`focusin`, onFocusIn)
      root.removeEventListener(`focusout`, onFocusOut)
      root.removeEventListener(`dragstart`, onDragStart)
      root.removeEventListener(`pointerover`, onPointerOver)
      root.removeEventListener(`pointerout`, onPointerOut)
    },
  }
}

/** The native's LAYOUT (inline: the theme recipe styles every part, these
 *  only place them; React style objects, Solid passes them through `css`).
 *  `pill` = style.json `zoomableImage.pill`. `flexShrink` comes last: jsdom
 *  re-appends it when Solid re-applies a style, so React's order holds. */
export function zoomableImageLayout(pill: { inset: number; percentMinWidth: number }) {
  return {
    root: { overflow: `clip`, userSelect: `none` },
    image: { position: `absolute`, left: 0, top: 0, display: `block`, maxWidth: `none`, maxHeight: `none`, transformOrigin: `0 0`, pointerEvents: `none`, userSelect: `none` },
    pill: { position: `absolute`, left: `50%`, bottom: pill.inset, transform: `translateX(-50%)`, zIndex: 1, display: `flex`, flexDirection: `row`, alignItems: `center`, whiteSpace: `nowrap`, cursor: `default`, transition: `opacity var(--xui-motion-standard) var(--xui-ease-standard,ease)` },
    pillHidden: { opacity: 0, pointerEvents: `none` },
    button: { display: `inline-flex`, alignItems: `center`, justifyContent: `center`, cursor: `pointer`, flexShrink: 0 },
    percent: { display: `inline-flex`, alignItems: `center`, justifyContent: `center`, cursor: `pointer`, minWidth: pill.percentMinWidth, fontVariantNumeric: `tabular-nums`, flexShrink: 0 },
    divider: { alignSelf: `center`, flexShrink: 0 },
  } as const
}

/** The settle spring of the native (the theme's standard motion). */
export const ZOOMABLE_IMAGE_NATIVE_TRANSITION = `transform var(--xui-motion-standard) var(--xui-ease-standard,ease)`
/** The primitive's (the app's motion tokens). */
export const ZOOMABLE_IMAGE_TRANSITION = `transform var(--motion-duration-standard,var(--xui-duration-standard,180ms)) var(--motion-ease-standard,cubic-bezier(0.2,0,0,1))`

/** The primitive's picture style before the controller paints it. */
export function zoomImageStyle(natural: ZoomSize | null): Record<string, string | number> {
  return natural ? { width: natural.width, height: natural.height } : {}
}

export const ZOOMABLE_IMAGE_CLASSES = {
  /** The viewport: as wide as its parent, the picture clipped to it. */
  root: `relative w-full min-w-0 overflow-hidden rounded-md select-none outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50`,
  image: `pointer-events-none absolute top-0 left-0 block max-h-none max-w-none origin-top-left select-none`,
  /** THE pill: one glass capsule bottom-centre, `pill.inset` above the edge. */
  pill: `absolute bottom-4 left-1/2 z-10 flex -translate-x-1/2 flex-row items-center gap-0.5 rounded-full border border-glass-stroke-card bg-popover p-1 whitespace-nowrap shadow-lg backdrop-blur-lg transition-opacity duration-standard ease-standard`,
  pillHidden: `pointer-events-none opacity-0`,
  /** − / + / fit: ghost icon buttons, 28 px circles (no border or fill of
   *  their own where the host has no preflight). */
  button: `size-7 rounded-full border-0 bg-transparent`,
  percent: `h-7 min-w-14 rounded-full border-0 bg-transparent px-2 tabular-nums`,
  divider: `mx-0.5 h-4 w-px shrink-0 bg-border`,
} as const
