// Round 1: shadcn's ScrollArea on Radix (the catalog's ScrollArea macro is a
// Box with overflowX/Y; this is the app-side primitive with styled bars).
export const SCROLL_AREA_CLASSES = {
  root: `relative`,
  viewport: `size-full rounded-[inherit] transition-[color,box-shadow] outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-1`,
  scrollbar: `flex touch-none p-px transition-colors select-none`,
  orientations: {
    vertical: `h-full w-2.5 border-l border-l-transparent`,
    horizontal: `h-2.5 flex-col border-t border-t-transparent`,
  },
  thumb: `relative flex-1 rounded-full bg-border`,
} as const

// FEED-131 — Radix ScrollArea's framework-free half for the Solid twin (React's
// Radix writes the same itself): the inline styles in Radix's order, the
// `type="scroll"` state machine and the thumb maths.

export type ScrollAreaDir = `ltr` | `rtl`
export type ScrollAreaOrientation = `vertical` | `horizontal`

/** Radix's viewport <style>: the native scrollbar hidden. */
export const SCROLL_AREA_VIEWPORT_CSS = `[data-radix-scroll-area-viewport]{scrollbar-width:none;-ms-overflow-style:none;-webkit-overflow-scrolling:touch;}[data-radix-scroll-area-viewport]::-webkit-scrollbar{display:none}`

export function scrollAreaRootStyle(cornerWidth: number, cornerHeight: number): Record<string, string> {
  return { position: `relative`, "--radix-scroll-area-corner-width": `${cornerWidth}px`, "--radix-scroll-area-corner-height": `${cornerHeight}px` }
}

export function scrollAreaViewportStyle(xEnabled: boolean, yEnabled: boolean): Record<string, string> {
  return { "overflow-x": xEnabled ? `scroll` : `hidden`, "overflow-y": yEnabled ? `scroll` : `hidden` }
}

/** The viewport's `display: table` sizer. */
export const SCROLL_AREA_CONTENT_STYLE = { "min-width": `100%`, display: `table` } as const

export function scrollAreaScrollbarStyle(orientation: ScrollAreaOrientation, dir: ScrollAreaDir, thumbSize: number): Record<string, string> {
  if (orientation === `horizontal`) {
    return { position: `absolute`, bottom: `0`, left: dir === `rtl` ? `var(--radix-scroll-area-corner-width)` : `0`, right: dir === `ltr` ? `var(--radix-scroll-area-corner-width)` : `0`, "--radix-scroll-area-thumb-width": `${thumbSize}px` }
  }
  const s: Record<string, string> = { position: `absolute`, top: `0` }
  if (dir === `ltr`) s.right = `0`
  if (dir === `rtl`) s.left = `0`
  s.bottom = `var(--radix-scroll-area-corner-height)`
  s[`--radix-scroll-area-thumb-height`] = `${thumbSize}px`
  return s
}

export const SCROLL_AREA_THUMB_STYLE = { width: `var(--radix-scroll-area-thumb-width)`, height: `var(--radix-scroll-area-thumb-height)` } as const

export function scrollAreaCornerStyle(width: number, height: number, dir: ScrollAreaDir): Record<string, string> {
  const s: Record<string, string> = { width: `${width}px`, height: `${height}px`, position: `absolute` }
  if (dir === `ltr`) s.right = `0`
  if (dir === `rtl`) s.left = `0`
  s.bottom = `0`
  return s
}

/** A scrollbar's / thumb's `data-state`. */
export const scrollAreaVisibility = (visible: boolean): `visible` | `hidden` => (visible ? `visible` : `hidden`)

export type ScrollAreaScrollState = `hidden` | `scrolling` | `interacting` | `idle`
/** Radix's `type="scroll"` machine. */
export const SCROLL_AREA_SCROLL_MACHINE: Record<ScrollAreaScrollState, Partial<Record<string, ScrollAreaScrollState>>> = {
  hidden: { SCROLL: `scrolling` },
  scrolling: { SCROLL_END: `idle`, POINTER_ENTER: `interacting` },
  interacting: { SCROLL: `interacting`, POINTER_LEAVE: `idle` },
  idle: { HIDE: `hidden`, SCROLL: `scrolling`, POINTER_ENTER: `interacting` },
}
export const SCROLL_AREA_SCROLL_INITIAL: ScrollAreaScrollState = `hidden`

export interface ScrollAreaSizes {
  content: number
  viewport: number
  scrollbar: { size: number; paddingStart: number; paddingEnd: number }
}

export const scrollAreaThumbRatio = (viewport: number, content: number): number => {
  const ratio = viewport / content
  return isNaN(ratio) ? 0 : ratio
}

/** Radix's `getThumbSize`: the ratio of the track, at least 18px. */
export function scrollAreaThumbSize(sizes: ScrollAreaSizes): number {
  const padding = sizes.scrollbar.paddingStart + sizes.scrollbar.paddingEnd
  return Math.max((sizes.scrollbar.size - padding) * scrollAreaThumbRatio(sizes.viewport, sizes.content), 18)
}

const linearScale = (input: [number, number], output: [number, number]) => (value: number) => {
  if (input[0] === input[1] || output[0] === output[1]) return output[0]
  return output[0] + ((output[1] - output[0]) / (input[1] - input[0])) * (value - input[0])
}

/** Radix's `getScrollPositionFromPointer`. */
export function scrollAreaScrollFromPointer(pointer: number, pointerOffset: number, sizes: ScrollAreaSizes, dir: ScrollAreaDir = `ltr`): number {
  const size = scrollAreaThumbSize(sizes)
  const offset = pointerOffset || size / 2
  const min = sizes.scrollbar.paddingStart + offset
  const max = sizes.scrollbar.size - sizes.scrollbar.paddingEnd - (size - offset)
  const maxScroll = sizes.content - sizes.viewport
  return linearScale([min, max], dir === `ltr` ? [0, maxScroll] : [maxScroll * -1, 0])(pointer)
}

/** Radix's `getThumbOffsetFromScroll`. */
export function scrollAreaThumbOffset(scroll: number, sizes: ScrollAreaSizes, dir: ScrollAreaDir = `ltr`): number {
  const size = scrollAreaThumbSize(sizes)
  const bar = sizes.scrollbar.size - sizes.scrollbar.paddingStart - sizes.scrollbar.paddingEnd
  const maxScroll = sizes.content - sizes.viewport
  const [lo, hi] = dir === `ltr` ? [0, maxScroll] : [maxScroll * -1, 0]
  const clamped = Math.min(hi, Math.max(lo, scroll))
  return linearScale([0, maxScroll], [0, bar - size])(clamped)
}
