// VAPP-87: the OVERLAY placement contract every renderer keeps (the React
// renderer through Radix's popper, the Rust core's layer manager, VAPP-86).
// One pure function: an anchor box, the overlay's size, the viewport, a
// preferred side → where the overlay lands. Rules: `OVERLAY_OFFSET` px from
// the anchor on the preferred side, centred along the other axis; FLIP to
// the opposite side when the preferred side has less room than the
// overlay needs and the opposite has more; then SHIFT along the cross axis
// to stay `OVERLAY_PADDING` inside the viewport. `fixtures/overlay-geometry.json`
// locks the results; the browser suite checks Radix lands within tolerance.

export const OVERLAY_OFFSET = 4
export const OVERLAY_PADDING = 8

export type OverlaySide = `top` | `right` | `bottom` | `left`
export type OverlayAlign = `start` | `center` | `end`

export interface Rect {
  x: number
  y: number
  width: number
  height: number
}
export interface Size {
  width: number
  height: number
}

export interface OverlayPlacement {
  x: number
  y: number
  side: OverlaySide
  /** True when the preferred side was swapped for its opposite. */
  flipped: boolean
}

const OPPOSITE: Record<OverlaySide, OverlaySide> = { top: `bottom`, bottom: `top`, left: `right`, right: `left` }

function room(side: OverlaySide, anchor: Rect, viewport: Size, padding: number): number {
  switch (side) {
    case `top`:
      return anchor.y - padding
    case `bottom`:
      return viewport.height - (anchor.y + anchor.height) - padding
    case `left`:
      return anchor.x - padding
    case `right`:
      return viewport.width - (anchor.x + anchor.width) - padding
  }
}

/** VAPP-281: where an overlay of `size` sits against `anchor` on `side`,
 *  `align`ed along it, `offset` away: NO flip and NO shift (a popup hosted
 *  OUTSIDE the surface; the host's window system constrains it, e.g. an
 *  `xdg_positioner`'s flip / slide). `fixtures/overlay-geometry.json`
 *  `hosted` locks it; the Rust core's `anchor_position` mirrors it. */
export function anchorPosition(anchor: Rect, size: Size, side: OverlaySide, align: OverlayAlign = `center`, offset: number = OVERLAY_OFFSET): { x: number; y: number } {
  const p = rawPosition(anchor, size, side, align, offset)
  return { x: round(p.x), y: round(p.y) }
}

function rawPosition(anchor: Rect, size: Size, side: OverlaySide, align: OverlayAlign, offset: number): { x: number; y: number } {
  const alongX = align === `start` ? anchor.x : align === `end` ? anchor.x + anchor.width - size.width : anchor.x + anchor.width / 2 - size.width / 2
  const alongY = align === `start` ? anchor.y : align === `end` ? anchor.y + anchor.height - size.height : anchor.y + anchor.height / 2 - size.height / 2
  switch (side) {
    case `top`:
      return { x: alongX, y: anchor.y - offset - size.height }
    case `bottom`:
      return { x: alongX, y: anchor.y + anchor.height + offset }
    case `left`:
      return { x: anchor.x - offset - size.width, y: alongY }
    case `right`:
      return { x: anchor.x + anchor.width + offset, y: alongY }
  }
}

export function placeOverlay(
  anchor: Rect,
  size: Size,
  viewport: Size,
  options: { side?: OverlaySide; align?: OverlayAlign; offset?: number; padding?: number } = {}
): OverlayPlacement {
  const preferred = options.side ?? `bottom`
  const align = options.align ?? `center`
  const offset = options.offset ?? OVERLAY_OFFSET
  const padding = options.padding ?? OVERLAY_PADDING
  const vertical = preferred === `top` || preferred === `bottom`
  const needed = (vertical ? size.height : size.width) + offset
  let side = preferred
  let flipped = false
  if (room(preferred, anchor, viewport, padding) < needed && room(OPPOSITE[preferred], anchor, viewport, padding) > room(preferred, anchor, viewport, padding)) {
    side = OPPOSITE[preferred]
    flipped = true
  }
  let { x, y } = rawPosition(anchor, size, side, align, offset)
  // Shift along the cross axis to stay inside the viewport.
  if (vertical === (side === `top` || side === `bottom`)) {
    if (side === `top` || side === `bottom`) x = Math.min(Math.max(x, padding), Math.max(padding, viewport.width - padding - size.width))
    else y = Math.min(Math.max(y, padding), Math.max(padding, viewport.height - padding - size.height))
  } else if (side === `top` || side === `bottom`) x = Math.min(Math.max(x, padding), Math.max(padding, viewport.width - padding - size.width))
  else y = Math.min(Math.max(y, padding), Math.max(padding, viewport.height - padding - size.height))
  return { x: round(x), y: round(y), side, flipped }
}

/** #EXP-1765: an overlay whose box reaches `inset` OUTSIDE its content on
 *  every side (the Select pill's popup: the IDE Popover's 1 px ring outside
 *  its content), placed in gpui-component's Popover order: the inset first
 *  (a `start` box begins `inset` before the anchor's start, an `end` one
 *  `inset` past its end), then `placeOverlay`'s flip and padding clamp, then
 *  the content's offset from the anchor's origin rounded to whole px, half
 *  away from zero (gpui's Positioner; may leave the box half a px past the
 *  padding, as the IDE's does). `fixtures/overlay-geometry.json` `inset`
 *  locks it; the Rust core's `place_inset_overlay` mirrors it. */
export function placeInsetOverlay(anchor: Rect, size: Size, viewport: Size, options: { side?: OverlaySide; align?: OverlayAlign; offset?: number; padding?: number }, inset: number): OverlayPlacement {
  const p = placeOverlay({ ...anchor, x: anchor.x - inset, width: anchor.width + 2 * inset }, size, viewport, options)
  return { ...p, x: round(snapInset(p.x, anchor.x, inset)), y: round(snapInset(p.y, anchor.y, inset)) }
}

/** #EXP-1629: `placeInsetOverlay`'s last step on one axis: a box placed at
 *  `at` whose content starts `inset` inside it, moved so the content's
 *  offset from the anchor's `origin` is whole px, half away from zero (the
 *  web renderers' popper applies it after its shift and flip). */
export function snapInset(at: number, origin: number, inset: number): number {
  return origin + roundAway(at + inset - origin) - inset
}

/** Rust's `f64::round`: half away from zero (`Math.round` takes -0.5 up). */
const roundAway = (n: number) => Math.sign(n) * Math.round(Math.abs(n))

const round = (n: number) => Math.round(n * 1000) / 1000
