// Round 8 (docs/round-8-contract.md §1): the Canvas node's pure core, shared
// by the three web renderers (the painters live in web/primitives; the WASM
// `CanvasPlayer` compiles and samples, src/motion). fixtures/canvas.json
// locks the fit mapping and the pointer inversion (src/canvas.test.ts; the
// engine's twin is eui-motion `canvas::fit_matrix` / `to_drawing`).

/** How the drawing fills its box (the catalog's `motionFit`). */
export type CanvasFit = `contain` | `cover` | `fill`
/** `[a, b, c, d, e, f]`: x' = a·x + c·y + e, y' = b·x + d·y + f. */
export type CanvasMatrix = readonly [number, number, number, number, number, number]

/** The events a Canvas with `pointer: true` fires, each with `{x, y}` in
 *  drawing units (`leave` carries none). */
export const CANVAS_EVENTS = [`press`, `move`, `leave`] as const
export type CanvasEvent = (typeof CANVAS_EVENTS)[number]

/** The packed `text` op code (eui-motion README › Packed frames, VERSION 2). */
export const CANVAS_TEXT_OP = 7
/** Packed text fields: font 0 sans | 1 mono, align 0 start | 1 center | 2 end,
 *  baseline 0 top | 1 middle | 2 alphabetic; weight is the CSS weight. */
export const CANVAS_TEXT_FONTS = [`sans`, `mono`] as const
export const CANVAS_TEXT_ALIGNS = [`start`, `center`, `end`] as const
export const CANVAS_TEXT_BASELINES = [`top`, `middle`, `alphabetic`] as const

/** The fit prop, defaulted (`contain`). */
export function canvasFit(value: unknown): CanvasFit {
  return value === `cover` || value === `fill` ? value : `contain`
}

/** THE fit mapping, drawing units → box px: contain = uniform min, centred;
 *  cover = uniform max, centred; fill = stretched from the top-left. A
 *  drawing without a size maps as the identity. */
export function canvasFitMatrix(width: number, height: number, boxWidth: number, boxHeight: number, fit: CanvasFit = `contain`): CanvasMatrix {
  if (!(width > 0) || !(height > 0)) return [1, 0, 0, 1, 0, 0]
  if (fit === `fill`) return [boxWidth / width, 0, 0, boxHeight / height, 0, 0]
  const s = fit === `cover` ? Math.max(boxWidth / width, boxHeight / height) : Math.min(boxWidth / width, boxHeight / height)
  return [s, 0, 0, s, (boxWidth - width * s) / 2, (boxHeight - height * s) / 2]
}

const round2 = (v: number): number => {
  const r = Math.round(v * 100) / 100
  return r === 0 ? 0 : r
}

/** A pointer at box px `(x, y)` (relative to the Canvas box's top-left) →
 *  drawing units: the fit mapping inverted, two decimals. What `press` and
 *  `move` carry. A point in a contain letterbox maps outside `0..size`
 *  (never clamped); null for an empty box. */
export function canvasToDrawing(x: number, y: number, width: number, height: number, boxWidth: number, boxHeight: number, fit: CanvasFit = `contain`): { x: number; y: number } | null {
  const [a, b, c, d, e, f] = canvasFitMatrix(width, height, boxWidth, boxHeight, fit)
  const det = a * d - b * c
  if (!Number.isFinite(det) || Math.abs(det) < 1e-12) return null
  const dx = x - e
  const dy = y - f
  return { x: round2((d * dx - c * dy) / det), y: round2((a * dy - b * dx) / det) }
}

/** The drawing's `size` when it is a valid `[w, h]` (positive), else null. */
export function canvasDeclaredSize(props: Record<string, unknown>): [number, number] | null {
  const s = props.size
  if (Array.isArray(s) && typeof s[0] === `number` && typeof s[1] === `number` && s[0] > 0 && s[1] > 0 && Number.isFinite(s[0]) && Number.isFinite(s[1])) return [s[0], s[1]]
  return null
}

/** THE Canvas box (layout.json `canvas`, the Motion rule): unstyled = the
 *  drawing's px width shrinking to the available width (`maxWidth: 100%`),
 *  its height from the aspect ratio; a style width or height wins (the
 *  aspect still fills the other edge); both set = no aspect. */
export function canvasBoxStyle(size: [number, number] | null, nodeStyle: unknown): Record<string, string> {
  const style = typeof nodeStyle === `object` && nodeStyle !== null ? (nodeStyle as Record<string, unknown>) : {}
  const hasW = style.width !== undefined
  const hasH = style.height !== undefined
  const out: Record<string, string> = {}
  if (!size) return out
  if (!hasW && !hasH) {
    out.width = `${size[0]}px`
    out.maxWidth = `100%`
  }
  if (!(hasW && hasH)) out.aspectRatio = `${size[0]} / ${size[1]}`
  return out
}

/** a11y.json: an img named by `label`; decorative (hidden) without one.
 *  `pointer` adds no role and no focus stop. */
export function canvasA11y(label: unknown): { role?: `img`; "aria-label"?: string; "aria-hidden"?: `true` } {
  return typeof label === `string` && label ? { role: `img`, "aria-label": label } : { "aria-hidden": `true` }
}

/** The document the engine compiles (`CanvasPlayer`): the node's size and shapes. */
export function canvasDocument(props: Record<string, unknown>): { size: unknown; shapes: unknown } {
  return { size: props.size, shapes: props.shapes }
}
