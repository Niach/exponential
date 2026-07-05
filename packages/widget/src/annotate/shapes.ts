// Annotation model + pure geometry, split out for unit tests. All shape
// coordinates live in IMAGE pixel space (the screenshot's native raster):
// flattening is exact and the editor's display scale is a pure transform.

export type AnnotationTool = `rect` | `pen` | `arrow`

export interface Point {
  x: number
  y: number
}

export interface AnnotationShape {
  tool: AnnotationTool
  // rect/arrow: [start, end]. pen: the sampled polyline (≥1 point while
  // drafting; committed shapes pass isDegenerate first).
  points: Point[]
}

export interface NormalizedRect {
  x: number
  y: number
  width: number
  height: number
}

// Corner pair → origin + positive extent, whatever the drag direction.
export function normalizeRect(a: Point, b: Point): NormalizedRect {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(a.x - b.x),
    height: Math.abs(a.y - b.y),
  }
}

// A crop is a rect in the ORIGINAL image's pixel space, or null for "keep the
// whole image". Resolving clamps it inside the real image bounds and treats a
// null / degenerate (<1px) crop as the full image, so flatten always has a
// valid box regardless of how the editor produced it.
export function resolveCrop(
  crop: NormalizedRect | null,
  imageWidth: number,
  imageHeight: number
): NormalizedRect {
  const full: NormalizedRect = {
    x: 0,
    y: 0,
    width: imageWidth,
    height: imageHeight,
  }
  if (!crop) return full
  const x = Math.min(Math.max(crop.x, 0), imageWidth)
  const y = Math.min(Math.max(crop.y, 0), imageHeight)
  const width = Math.min(crop.width, imageWidth - x)
  const height = Math.min(crop.height, imageHeight - y)
  if (width < 1 || height < 1) return full
  return { x, y, width, height }
}

// One stroke width per image (~0.35% of the long edge, min 3px) so all
// shapes on a screenshot read consistently at both full and thumbnail size.
export function strokeWidthFor(
  imageWidth: number,
  imageHeight: number
): number {
  const longest = Math.max(imageWidth, imageHeight, 1)
  return Math.max(3, Math.round(longest * 0.0035))
}

export interface ArrowHead {
  // Where the shaft stroke should stop so it doesn't poke through the head.
  shaftEnd: Point
  tip: Point
  left: Point
  right: Point
}

const arrowSpread = Math.PI / 7

export function arrowHeadLength(strokeWidth: number): number {
  return Math.max(10, strokeWidth * 4)
}

// Filled-triangle head for an arrow from `from` to `to`.
export function arrowHead(
  from: Point,
  to: Point,
  strokeWidth: number
): ArrowHead {
  const angle = Math.atan2(to.y - from.y, to.x - from.x)
  const length = arrowHeadLength(strokeWidth)
  const wing = (spread: number): Point => ({
    x: to.x - length * Math.cos(angle + spread),
    y: to.y - length * Math.sin(angle + spread),
  })
  return {
    tip: to,
    left: wing(-arrowSpread),
    right: wing(arrowSpread),
    shaftEnd: {
      x: to.x - length * 0.6 * Math.cos(angle),
      y: to.y - length * 0.6 * Math.sin(angle),
    },
  }
}

export function distance(a: Point, b: Point): number {
  return Math.hypot(a.x - b.x, a.y - b.y)
}

// Accidental clicks shouldn't commit invisible shapes: rect/arrow need a
// real drag; a pen stroke needs at least two samples (a tiny scribble is a
// legitimate dot marker, so no length threshold there).
export function isDegenerate(shape: AnnotationShape, minDragPx = 4): boolean {
  if (shape.tool === `pen`) return shape.points.length < 2
  if (shape.points.length < 2) return true
  return distance(shape.points[0], shape.points[1]) < minDragPx
}

export function clampPoint(
  point: Point,
  width: number,
  height: number
): Point {
  return {
    x: Math.min(Math.max(point.x, 0), width),
    y: Math.min(Math.max(point.y, 0), height),
  }
}
