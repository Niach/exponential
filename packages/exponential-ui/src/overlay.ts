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
  let x: number
  let y: number
  const alongX = (): number => (align === `start` ? anchor.x : align === `end` ? anchor.x + anchor.width - size.width : anchor.x + anchor.width / 2 - size.width / 2)
  const alongY = (): number => (align === `start` ? anchor.y : align === `end` ? anchor.y + anchor.height - size.height : anchor.y + anchor.height / 2 - size.height / 2)
  switch (side) {
    case `top`:
      x = alongX()
      y = anchor.y - offset - size.height
      break
    case `bottom`:
      x = alongX()
      y = anchor.y + anchor.height + offset
      break
    case `left`:
      x = anchor.x - offset - size.width
      y = alongY()
      break
    case `right`:
      x = anchor.x + anchor.width + offset
      y = alongY()
      break
  }
  // Shift along the cross axis to stay inside the viewport.
  if (vertical === (side === `top` || side === `bottom`)) {
    if (side === `top` || side === `bottom`) x = Math.min(Math.max(x, padding), Math.max(padding, viewport.width - padding - size.width))
    else y = Math.min(Math.max(y, padding), Math.max(padding, viewport.height - padding - size.height))
  } else if (side === `top` || side === `bottom`) x = Math.min(Math.max(x, padding), Math.max(padding, viewport.width - padding - size.width))
  else y = Math.min(Math.max(y, padding), Math.max(padding, viewport.height - padding - size.height))
  return { x: round(x), y: round(y), side, flipped }
}

const round = (n: number) => Math.round(n * 1000) / 1000
