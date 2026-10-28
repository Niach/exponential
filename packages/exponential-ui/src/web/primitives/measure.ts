// NEW-82: element measurements in LAYOUT px, framework-free (the React and
// Solid renderers share them). `getBoundingClientRect()` is in VISUAL px: it
// includes CSS `zoom` and transforms, while everything a painter lays out
// (an SVG viewBox, a breakpoint, a translateY, a scrollTop) is in the
// element's own unzoomed CSS px. Inside a `zoom: 0.76` phone frame a 200 px
// box reads 152 from the rect.
//
// The rect stays the source whenever it agrees with the layout box (no
// zoom, no transform): its FRACTIONAL width is what the renderers measured
// before, so an unscaled page measures byte-identically. A scaled element
// reads the ResizeObserver entry's `borderBoxSize` (fractional, unzoomed),
// else the rect divided by `currentCSSZoom` when that alone explains the
// difference, else `offsetWidth`/`offsetHeight` (whole px). The two
// divided reads snap to Blink's 1/64 px LayoutUnit: un-zooming is a float
// division (700 px under `zoom: 0.76` reads 700.0000087…).

export interface LayoutSize {
  width: number
  height: number
}

/** Within a pixel: `offsetWidth` is the layout box snapped to whole px. */
const near = (a: number, b: number) => Math.abs(a - b) < 1
const unit = (v: number) => Math.round(v * 64) / 64
const sized = (width: number, height: number): LayoutSize => ({ width: unit(width), height: unit(height) })

/** An element's border box in layout (unzoomed, untransformed) CSS px.
 *  `entry` = the ResizeObserver entry of this element, when there is one. */
export function layoutSize(el: HTMLElement, entry?: ResizeObserverEntry): LayoutSize {
  const r = el.getBoundingClientRect()
  const ow = el.offsetWidth
  const oh = el.offsetHeight
  // No offset box (jsdom, no layout): the rect is all there is.
  if (ow === 0 && oh === 0) return { width: r.width, height: r.height }
  if (near(r.width, ow) && near(r.height, oh)) return { width: r.width, height: r.height }
  const box = entry?.borderBoxSize?.[0]
  if (box) {
    if (near(box.inlineSize, ow) && near(box.blockSize, oh)) return sized(box.inlineSize, box.blockSize)
    // A vertical writing mode: inline = height.
    if (near(box.blockSize, ow) && near(box.inlineSize, oh)) return sized(box.blockSize, box.inlineSize)
  }
  const zoom = (el as HTMLElement & { currentCSSZoom?: number }).currentCSSZoom
  if (zoom && zoom !== 1 && near(r.width / zoom, ow) && near(r.height / zoom, oh)) return sized(r.width / zoom, r.height / zoom)
  return { width: ow, height: oh }
}

/** Visual px per layout px on one axis: 1 when they agree within a pixel
 *  (an unscaled element maps byte-identically), else `visual / layout`.
 *  Divide a visual delta (a pointer offset, a rect difference) by it. */
export function visualScale(visual: number, layout: number): number {
  return layout > 0 && !near(visual, layout) ? visual / layout : 1
}

/** `visualScale` of an element: its rect over its offset box. */
export function elementScale(el: HTMLElement, rect: DOMRect = el.getBoundingClientRect()): { x: number; y: number } {
  return { x: visualScale(rect.width, el.offsetWidth), y: visualScale(rect.height, el.offsetHeight) }
}
