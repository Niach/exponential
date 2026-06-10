// Canvas rendering for annotation shapes — shared by the editor's live
// overlay and the final flatten so what you draw is exactly what ships.
import {
  type AnnotationShape,
  arrowHead,
  normalizeRect,
} from "./shapes"

// marker.io-style red: high contrast on most screenshots, matches the
// widget's destructive token.
export const annotationColor = `#ef4444`

function setStroke(
  context: CanvasRenderingContext2D,
  strokeWidth: number
): void {
  context.strokeStyle = annotationColor
  context.fillStyle = annotationColor
  context.lineWidth = strokeWidth
  context.lineCap = `round`
  context.lineJoin = `round`
}

function drawPen(
  context: CanvasRenderingContext2D,
  shape: AnnotationShape,
  strokeWidth: number
): void {
  const points = shape.points
  if (points.length === 0) return
  if (points.length === 1) {
    // In-progress stroke before the first move: a dot.
    context.beginPath()
    context.arc(points[0].x, points[0].y, strokeWidth / 2, 0, Math.PI * 2)
    context.fill()
    return
  }
  // Quadratic midpoint smoothing keeps fast mouse strokes from looking
  // like polygons without resampling the input.
  context.beginPath()
  context.moveTo(points[0].x, points[0].y)
  for (let i = 1; i < points.length - 1; i++) {
    const midX = (points[i].x + points[i + 1].x) / 2
    const midY = (points[i].y + points[i + 1].y) / 2
    context.quadraticCurveTo(points[i].x, points[i].y, midX, midY)
  }
  const last = points[points.length - 1]
  context.lineTo(last.x, last.y)
  context.stroke()
}

function drawRect(
  context: CanvasRenderingContext2D,
  shape: AnnotationShape
): void {
  if (shape.points.length < 2) return
  const rect = normalizeRect(shape.points[0], shape.points[1])
  context.beginPath()
  context.rect(rect.x, rect.y, rect.width, rect.height)
  context.stroke()
}

function drawArrow(
  context: CanvasRenderingContext2D,
  shape: AnnotationShape,
  strokeWidth: number
): void {
  if (shape.points.length < 2) return
  const [from, to] = shape.points
  const head = arrowHead(from, to, strokeWidth)
  context.beginPath()
  context.moveTo(from.x, from.y)
  context.lineTo(head.shaftEnd.x, head.shaftEnd.y)
  context.stroke()
  context.beginPath()
  context.moveTo(head.tip.x, head.tip.y)
  context.lineTo(head.left.x, head.left.y)
  context.lineTo(head.right.x, head.right.y)
  context.closePath()
  context.fill()
}

export function drawShape(
  context: CanvasRenderingContext2D,
  shape: AnnotationShape,
  strokeWidth: number
): void {
  setStroke(context, strokeWidth)
  if (shape.tool === `pen`) drawPen(context, shape, strokeWidth)
  else if (shape.tool === `rect`) drawRect(context, shape)
  else drawArrow(context, shape, strokeWidth)
}

export function drawShapes(
  context: CanvasRenderingContext2D,
  shapes: readonly AnnotationShape[],
  strokeWidth: number
): void {
  for (const shape of shapes) drawShape(context, shape, strokeWidth)
}
