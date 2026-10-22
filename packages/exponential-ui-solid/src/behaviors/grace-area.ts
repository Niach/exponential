// FEED-131: Radix's pointer grace area (Tooltip's hoverable content, the
// Menu's submenu): the polygon from the exit point to the target's box.

export type Point = { x: number; y: number }
/** Is the point inside the polygon (ray casting, Radix's `isPointInPolygon`)? */
export function inPolygon(point: Point, polygon: Point[]): boolean {
  let inside = false
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const a = polygon[i]
    const b = polygon[j]
    if (a.y > point.y !== b.y > point.y && point.x < ((b.x - a.x) * (point.y - a.y)) / (b.y - a.y) + a.x) inside = !inside
  }
  return inside
}
/** The convex hull of points (Andrew's monotone chain, Radix's `getHull`). */
export function hull(points: Point[]): Point[] {
  const sorted = [...points].sort((a, b) => (a.x < b.x ? -1 : a.x > b.x ? 1 : a.y < b.y ? -1 : a.y > b.y ? 1 : 0))
  if (sorted.length <= 1) return sorted
  const cross = (o: Point, a: Point, b: Point) => (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
  const upper: Point[] = []
  for (const p of sorted) {
    while (upper.length >= 2 && cross(upper[upper.length - 2], upper[upper.length - 1], p) >= 0) upper.pop()
    upper.push(p)
  }
  upper.pop()
  const lower: Point[] = []
  for (let i = sorted.length - 1; i >= 0; i--) {
    const p = sorted[i]
    while (lower.length >= 2 && cross(lower[lower.length - 2], lower[lower.length - 1], p) >= 0) lower.pop()
    lower.push(p)
  }
  lower.pop()
  return upper.length === 1 && lower.length === 1 && upper[0].x === lower[0].x && upper[0].y === lower[0].y ? upper : upper.concat(lower)
}
/** The grace area from the exit point to a hover target's box. */
export function graceArea(exit: Point, from: DOMRect, to: DOMRect): Point[] {
  const padding = 5
  const top = Math.abs(from.top - exit.y)
  const bottom = Math.abs(from.bottom - exit.y)
  const right = Math.abs(from.right - exit.x)
  const left = Math.abs(from.left - exit.x)
  const m = Math.min(top, bottom, right, left)
  const padded: Point[] =
    m === left
      ? [{ x: exit.x + padding, y: exit.y - padding }, { x: exit.x + padding, y: exit.y + padding }]
      : m === right
        ? [{ x: exit.x - padding, y: exit.y - padding }, { x: exit.x - padding, y: exit.y + padding }]
        : m === top
          ? [{ x: exit.x - padding, y: exit.y + padding }, { x: exit.x + padding, y: exit.y + padding }]
          : [{ x: exit.x - padding, y: exit.y - padding }, { x: exit.x + padding, y: exit.y - padding }]
  return hull([...padded, { x: to.left, y: to.top }, { x: to.right, y: to.top }, { x: to.right, y: to.bottom }, { x: to.left, y: to.bottom }])
}
