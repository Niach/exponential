// FEED-131: Radix `Popper` on @floating-ui/dom: Radix's middleware (offset
// → shift(limitShift) → flip → size → transformOrigin, `strategy: fixed`),
// the `data-radix-popper-content-wrapper` and its inline style in Radix's
// order, the content's `--radix-<kind>-*` variables. `popperWrapper` is the
// Select's and Combobox's variant (a reactive `style`, placed once mounted),
// kept as it was ported.

import { createEffect, createMemo, createSignal, onCleanup, onMount, untrack, type Accessor, type JSX } from "solid-js"
import { insert } from "solid-js/web"
import { autoUpdate, computePosition, flip, limitShift, offset, shift, size, type Middleware, type Placement, type VirtualElement } from "@floating-ui/dom"
import { css, type StyleInput } from "../style"
import { styleWriter, type StyleObject } from "./style-writer"

export type Side = `top` | `right` | `bottom` | `left`
export type Align = `start` | `center` | `end`

/** Radix Popper's `transformOrigin` middleware (no arrow: always hidden). */
export const transformOrigin: Middleware = {
  name: `transformOrigin`,
  fn(data) {
    const [placedSide, placedAlign = `center`] = data.placement.split(`-`) as [Side, Align | undefined]
    const noArrowAlign = { start: `0%`, center: `50%`, end: `100%` }[placedAlign]
    let x = ``
    let y = ``
    if (placedSide === `bottom`) {
      x = noArrowAlign
      y = `${-0}px`
    } else if (placedSide === `top`) {
      x = noArrowAlign
      y = `${data.rects.floating.height + 0}px`
    } else if (placedSide === `right`) {
      x = `${-0}px`
      y = noArrowAlign
    } else if (placedSide === `left`) {
      x = `${data.rects.floating.width + 0}px`
      y = noArrowAlign
    }
    return { data: { x, y } }
  },
}

export interface PopperState {
  placedSide: Accessor<Side>
  placedAlign: Accessor<Align>
  isPositioned: Accessor<boolean>
  /** The content element (its computed `z-index` goes on the wrapper). */
  setContent: (el: HTMLElement) => void
}

/** Radix `PopperContent`: the `data-radix-popper-content-wrapper` (fixed,
 *  translated, `min-width: max-content`, the content's z-index, the
 *  `--radix-popper-*` variables) around the content `children(state)`.
 *  Placement = floating-ui with Radix's middleware, auto-updated. */
export function Popper(props: { anchor: Element | VirtualElement | null | undefined; side: Side; align: Align; sideOffset: number; alignOffset?: number; collisionPadding: number; dir?: string; children: (state: PopperState) => JSX.Element }): JSX.Element {
  const desired = () => (props.align === `center` ? props.side : `${props.side}-${props.align}`) as Placement
  const [placement, setPlacement] = createSignal<Placement>(untrack(desired))
  const [isPositioned, setPositioned] = createSignal(false)
  const [content, setContent] = createSignal<HTMLElement>()
  let pos = { x: 0, y: 0, origin: undefined as { x: string; y: string } | undefined }
  let floatingSet = false
  let zIndex: string | undefined
  let disposed = false
  const wrapper = (<div data-radix-popper-content-wrapper="" dir={props.dir as `ltr` | `rtl` | undefined} />) as HTMLDivElement
  const writeWrapper = styleWriter(wrapper)
  const dpr = () => wrapper.ownerDocument.defaultView?.devicePixelRatio || 1
  const roundByDPR = (v: number) => Math.round(v * dpr()) / dpr()
  const paintWrapper = () => {
    const s: StyleObject = { position: `fixed`, left: `0`, top: `0` }
    // floating-ui's floatingStyles: the transform (+ will-change at DPR ≥ 1.5) once the floating element is known.
    if (floatingSet) {
      s.transform = `translate(${roundByDPR(pos.x)}px, ${roundByDPR(pos.y)}px)`
      if (dpr() >= 1.5) s[`will-change`] = `transform`
    }
    s.transform = untrack(isPositioned) ? s.transform : `translate(0, -200%)`
    s[`min-width`] = `max-content`
    s[`z-index`] = zIndex || undefined
    s[`--radix-popper-transform-origin`] = [pos.origin?.x, pos.origin?.y].join(` `)
    writeWrapper(s)
  }
  paintWrapper()
  const state: PopperState = { placedSide: () => placement().split(`-`)[0] as Side, placedAlign: () => (placement().split(`-`)[1] ?? `center`) as Align, isPositioned, setContent }
  insert(wrapper, untrack(() => props.children(state)))
  createEffect(() => {
    const anchor = props.anchor
    const el = content()
    if (!anchor || !el) return
    untrack(() => {
      floatingSet = true
      paintWrapper()
      zIndex = getComputedStyle(el).zIndex
      paintWrapper()
    })
    const padding = props.collisionPadding
    const detect = { padding, boundary: [] as Element[], altBoundary: false }
    const middleware = [
      offset({ mainAxis: props.sideOffset, alignmentAxis: props.alignOffset ?? 0 }),
      shift({ mainAxis: true, crossAxis: false, limiter: limitShift(), ...detect }),
      flip({ ...detect }),
      size({
        ...detect,
        apply: ({ elements, rects, availableWidth, availableHeight }) => {
          const st = elements.floating.style
          st.setProperty(`--radix-popper-available-width`, `${availableWidth}px`)
          st.setProperty(`--radix-popper-available-height`, `${availableHeight}px`)
          st.setProperty(`--radix-popper-anchor-width`, `${rects.reference.width}px`)
          st.setProperty(`--radix-popper-anchor-height`, `${rects.reference.height}px`)
        },
      }),
      transformOrigin,
    ]
    const placementNow = desired()
    const update = () => {
      void computePosition(anchor, wrapper, { strategy: `fixed`, placement: placementNow, middleware }).then((d) => {
        if (disposed) return
        const origin = d.middlewareData.transformOrigin as { x: string; y: string } | undefined
        const same = untrack(isPositioned) && pos.x === d.x && pos.y === d.y && pos.origin?.x === origin?.x && pos.origin?.y === origin?.y && untrack(placement) === d.placement
        if (same) return
        pos = { x: d.x, y: d.y, origin }
        setPlacement(d.placement)
        setPositioned(true)
        paintWrapper()
      })
    }
    const stop = autoUpdate(anchor, wrapper, update, { animationFrame: false })
    onCleanup(stop)
  })
  onCleanup(() => (disposed = true))
  return wrapper
}

/** The content's own style the way Radix composes it: `prefix` (a menu's
 *  `outline`), the dismissable layer's `pointer-events`, the popper's
 *  `--radix-<kind>-*` variables, then `animation: none` until placed. */
export function popperContentStyle(kind: string, trigger: string, pointerEvents: string | undefined, positioned: boolean, prefix: StyleObject = {}): StyleObject {
  return {
    ...prefix,
    "pointer-events": pointerEvents,
    [`--radix-${kind}-content-transform-origin`]: `var(--radix-popper-transform-origin)`,
    [`--radix-${kind}-content-available-width`]: `var(--radix-popper-available-width)`,
    [`--radix-${kind}-content-available-height`]: `var(--radix-popper-available-height)`,
    [`--radix-${kind}-${trigger}-width`]: `var(--radix-popper-anchor-width)`,
    [`--radix-${kind}-${trigger}-height`]: `var(--radix-popper-anchor-height)`,
    animation: positioned ? undefined : `none`,
  }
}

export interface Placed {
  side: string
  align: string
  positioned: boolean
}

/** Radix's PopperContent: the fixed wrapper (`data-radix-popper-content-
 *  wrapper`, floating-ui's translate, `--radix-popper-*`), the content's
 *  placed side/align. `content` renders the content element. */
export function popperWrapper(opts: { anchor: () => HTMLElement | undefined; sideOffset: number; align: `start` | `center` | `end`; collisionPadding: number; dir?: () => `ltr` | `rtl` | `auto` | undefined; onPlaced?: () => void; content: (placed: Accessor<Placed>, ref: (el: HTMLElement) => void) => JSX.Element }): HTMLElement {
  const [pos, setPos] = createSignal<{ x: number; y: number; placement: Placement; origin: [string, string] } | null>(null)
  const [zIndex, setZIndex] = createSignal<string>()
  let contentEl: HTMLElement | undefined
  const dpr = () => (typeof window === `undefined` ? 1 : window.devicePixelRatio || 1)
  const round = (v: number) => Math.round(v * dpr()) / dpr()
  const placed = createMemo<Placed>(() => {
    const p = pos()
    const [side, align = `center`] = (p?.placement ?? `bottom-${opts.align}`).split(`-`)
    return { side, align: opts.align === `center` && !p ? `center` : align, positioned: p !== null }
  })
  const style = () => {
    const p = pos()
    const s: StyleInput = { position: `fixed`, left: 0, top: 0, transform: p ? `translate(${round(p.x)}px, ${round(p.y)}px)` : `translate(0, -200%)` }
    if (dpr() >= 1.5) s.willChange = `transform`
    s.minWidth = `max-content`
    s.zIndex = zIndex()
    s[`--radix-popper-transform-origin`] = p ? p.origin.join(` `) : ` `
    return css(s)
  }
  const wrapper = (
    <div data-radix-popper-content-wrapper="" style={style()} dir={opts.dir?.()}>
      {opts.content(placed, (el) => (contentEl = el))}
    </div>
  ) as HTMLElement
  onMount(() => {
    if (contentEl) setZIndex(getComputedStyle(contentEl).zIndex)
    const anchor = opts.anchor()
    if (!anchor) return
    const detect = { padding: opts.collisionPadding, boundary: [] as Element[], altBoundary: false }
    let alive = true
    const update = () =>
      void computePosition(anchor, wrapper, {
        strategy: `fixed`,
        placement: (opts.align === `center` ? `bottom` : `bottom-${opts.align}`) as Placement,
        middleware: [
          offset({ mainAxis: opts.sideOffset, alignmentAxis: 0 }),
          shift({ mainAxis: true, crossAxis: false, limiter: limitShift(), ...detect }),
          flip({ ...detect }),
          size({
            ...detect,
            apply: ({ elements, rects, availableWidth, availableHeight }) => {
              const st = elements.floating.style
              st.setProperty(`--radix-popper-available-width`, `${availableWidth}px`)
              st.setProperty(`--radix-popper-available-height`, `${availableHeight}px`)
              st.setProperty(`--radix-popper-anchor-width`, `${rects.reference.width}px`)
              st.setProperty(`--radix-popper-anchor-height`, `${rects.reference.height}px`)
            },
          }),
          transformOrigin,
        ],
      }).then((r) => {
        if (!alive) return
        const first = pos() === null
        const origin = r.middlewareData.transformOrigin as { x: string; y: string } | undefined
        setPos({ x: r.x, y: r.y, placement: r.placement, origin: [origin?.x ?? ``, origin?.y ?? ``] })
        if (first) opts.onPlaced?.()
      })
    const stop = autoUpdate(anchor, wrapper, update, { animationFrame: false })
    onCleanup(() => {
      alive = false
      stop()
    })
  })
  return wrapper
}
