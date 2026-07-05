import { useCallback, useEffect, useRef, useState } from "preact/hooks"
import {
  type AnnotationShape,
  type AnnotationTool,
  type NormalizedRect,
  type Point,
  clampPoint,
  isDegenerate,
  normalizeRect,
  strokeWidthFor,
} from "../annotate/shapes"
import { drawShape, drawShapes } from "../annotate/draw"

// The editor's tool set is the persisted annotation tools plus `crop`, which
// is NOT a drawn shape but a separate rect baked into the image at flatten.
type EditorTool = AnnotationTool | `crop`

const rectIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="6" width="16" height="12" rx="1"/></svg>`
const penIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 19c3-6 5-9 7-9s2 7 4 7 4-8 7-12"/></svg>`
const arrowIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 19 19 5"/><path d="M9 5h10v10"/></svg>`
const cropIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 2v14a2 2 0 0 0 2 2h14"/><path d="M18 22V8a2 2 0 0 0-2-2H2"/></svg>`
const undoIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 7v6h6"/><path d="M3 13a9 9 0 1 0 3-7.7L3 7"/></svg>`
const trashIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 6h18"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>`
const cropOffIconSvg = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 2v14a2 2 0 0 0 2 2h14"/><path d="M18 22V8a2 2 0 0 0-2-2H2"/><path d="m2 2 20 20"/></svg>`

const tools: Array<{
  tool: EditorTool
  label: string
  icon: string
}> = [
  { tool: `rect`, label: `Rectangle`, icon: rectIconSvg },
  { tool: `pen`, label: `Free line`, icon: penIconSvg },
  { tool: `arrow`, label: `Arrow`, icon: arrowIconSvg },
  { tool: `crop`, label: `Crop`, icon: cropIconSvg },
]

// Minimum crop drag (image px) below which a click is ignored rather than
// producing a sliver crop.
const minCropPx = 8

// Editor-only preview of what a crop keeps: dim everything outside the rect
// and dash its outline. The exported image is physically cropped at flatten,
// so this overlay is never baked into the screenshot.
function drawCropOverlay(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
  rect: NormalizedRect
): void {
  const right = rect.x + rect.width
  const bottom = rect.y + rect.height
  context.save()
  context.fillStyle = `rgba(0, 0, 0, 0.55)`
  context.fillRect(0, 0, width, rect.y)
  context.fillRect(0, bottom, width, height - bottom)
  context.fillRect(0, rect.y, rect.x, rect.height)
  context.fillRect(right, rect.y, width - right, rect.height)
  const line = Math.max(2, Math.round(Math.max(width, height) * 0.002))
  context.strokeStyle = `#ffffff`
  context.lineWidth = line
  context.setLineDash([line * 3, line * 3])
  context.strokeRect(rect.x, rect.y, rect.width, rect.height)
  context.restore()
}

// Full-screen annotation editor over the captured screenshot. The canvas
// backing store stays at the image's native resolution; pointer coordinates
// are mapped from display space, so the flattened output matches the editor
// exactly.
export function Annotator(props: {
  imageUrl: string
  initialShapes: readonly AnnotationShape[]
  initialCrop: NormalizedRect | null
  onCancel(): void
  onSave(shapes: AnnotationShape[], crop: NormalizedRect | null): void
}) {
  const [tool, setTool] = useState<EditorTool>(`rect`)
  const [shapes, setShapes] = useState<AnnotationShape[]>([
    ...props.initialShapes,
  ])
  const [crop, setCrop] = useState<NormalizedRect | null>(props.initialCrop)
  const [image, setImage] = useState<HTMLImageElement | null>(null)
  const [display, setDisplay] = useState<{
    width: number
    height: number
  } | null>(null)

  const rootRef = useRef<HTMLDivElement>(null)
  const stageRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const draftRef = useRef<AnnotationShape | null>(null)
  // In-progress crop drag as a corner pair, in image px.
  const cropDraftRef = useRef<[Point, Point] | null>(null)
  const shapesRef = useRef(shapes)
  shapesRef.current = shapes
  const cropRef = useRef(crop)
  cropRef.current = crop

  useEffect(() => {
    const element = new Image()
    element.onload = () => setImage(element)
    // The object URL is owned by App and outlives this editor.
    element.src = props.imageUrl
  }, [props.imageUrl])

  // Fit the image into the stage, downscale-only.
  useEffect(() => {
    const stage = stageRef.current
    if (!stage || !image) return
    const updateFit = () => {
      const availableWidth = Math.max(stage.clientWidth - 16, 50)
      const availableHeight = Math.max(stage.clientHeight - 16, 50)
      const scale = Math.min(
        availableWidth / image.naturalWidth,
        availableHeight / image.naturalHeight,
        1
      )
      setDisplay({
        width: Math.round(image.naturalWidth * scale),
        height: Math.round(image.naturalHeight * scale),
      })
    }
    updateFit()
    const observer = new ResizeObserver(updateFit)
    observer.observe(stage)
    return () => observer.disconnect()
  }, [image])

  const redraw = useCallback(() => {
    const canvas = canvasRef.current
    if (!canvas || !image) return
    const context = canvas.getContext(`2d`)
    if (!context) return
    context.clearRect(0, 0, canvas.width, canvas.height)
    context.drawImage(image, 0, 0)
    const strokeWidth = strokeWidthFor(canvas.width, canvas.height)
    drawShapes(context, shapesRef.current, strokeWidth)
    if (draftRef.current) drawShape(context, draftRef.current, strokeWidth)
    // A live crop drag preempts the committed crop for the overlay.
    const draftCrop = cropDraftRef.current
    const cropRect = draftCrop
      ? normalizeRect(draftCrop[0], draftCrop[1])
      : cropRef.current
    if (cropRect) drawCropOverlay(context, canvas.width, canvas.height, cropRect)
  }, [image])

  useEffect(() => {
    redraw()
  }, [redraw, shapes, crop, display])

  useEffect(() => {
    rootRef.current?.focus()
  }, [])

  const toImagePoint = (event: PointerEvent): Point | null => {
    const canvas = canvasRef.current
    if (!canvas || !image) return null
    const rect = canvas.getBoundingClientRect()
    if (rect.width === 0 || rect.height === 0) return null
    return clampPoint(
      {
        x: ((event.clientX - rect.left) * canvas.width) / rect.width,
        y: ((event.clientY - rect.top) * canvas.height) / rect.height,
      },
      canvas.width,
      canvas.height
    )
  }

  const onPointerDown = (event: PointerEvent) => {
    if (event.pointerType === `mouse` && event.button !== 0) return
    const point = toImagePoint(event)
    if (!point) return
    event.preventDefault()
    canvasRef.current?.setPointerCapture(event.pointerId)
    if (tool === `crop`) {
      cropDraftRef.current = [point, point]
    } else {
      draftRef.current = { tool, points: [point] }
    }
    redraw()
  }

  const onPointerMove = (event: PointerEvent) => {
    const canvas = canvasRef.current
    if (!canvas) return
    const cropDraft = cropDraftRef.current
    if (cropDraft) {
      const point = toImagePoint(event)
      if (!point) return
      event.preventDefault()
      cropDraft[1] = point
      redraw()
      return
    }
    const draft = draftRef.current
    if (!draft) return
    const point = toImagePoint(event)
    if (!point) return
    event.preventDefault()
    if (draft.tool === `pen`) {
      // Sample at ~1 display pixel so strokes stay light at high zoom-out.
      const rect = canvas.getBoundingClientRect()
      const minStep = rect.width > 0 ? canvas.width / rect.width : 1
      const last = draft.points[draft.points.length - 1]
      if (Math.hypot(point.x - last.x, point.y - last.y) < minStep) return
      draft.points.push(point)
    } else {
      draft.points = [draft.points[0], point]
    }
    redraw()
  }

  const commitDraft = () => {
    const cropDraft = cropDraftRef.current
    if (cropDraft) {
      cropDraftRef.current = null
      const rect = normalizeRect(cropDraft[0], cropDraft[1])
      // Ignore an accidental click / sliver; keep any existing crop.
      if (rect.width >= minCropPx && rect.height >= minCropPx) {
        setCrop(rect)
      } else {
        redraw()
      }
      return
    }
    const draft = draftRef.current
    draftRef.current = null
    if (draft && !isDegenerate(draft)) {
      setShapes((previous) => [...previous, draft])
    } else {
      redraw()
    }
  }

  const onPointerCancel = () => {
    draftRef.current = null
    cropDraftRef.current = null
    redraw()
  }

  const undo = () => setShapes((previous) => previous.slice(0, -1))

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === `Escape`) {
      event.stopPropagation()
      props.onCancel()
      return
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === `z`) {
      event.preventDefault()
      undo()
    }
  }

  return (
    <div
      ref={rootRef}
      className="exp-annotator"
      role="dialog"
      aria-modal="true"
      aria-label="Annotate screenshot"
      tabIndex={-1}
      onKeyDown={onKeyDown}
    >
      <div className="exp-ann-toolbar">
        {tools.map((entry) => (
          <button
            key={entry.tool}
            type="button"
            className="exp-tool"
            aria-label={entry.label}
            title={entry.label}
            aria-pressed={tool === entry.tool}
            onClick={() => setTool(entry.tool)}
            dangerouslySetInnerHTML={{ __html: entry.icon }}
          />
        ))}
        <span className="exp-ann-sep" />
        <button
          type="button"
          className="exp-tool"
          aria-label="Undo"
          title="Undo"
          disabled={shapes.length === 0}
          onClick={undo}
          dangerouslySetInnerHTML={{ __html: undoIconSvg }}
        />
        <button
          type="button"
          className="exp-tool"
          aria-label="Clear annotations"
          title="Clear annotations"
          disabled={shapes.length === 0}
          onClick={() => setShapes([])}
          dangerouslySetInnerHTML={{ __html: trashIconSvg }}
        />
        <button
          type="button"
          className="exp-tool"
          aria-label="Remove crop"
          title="Remove crop"
          disabled={!crop}
          onClick={() => setCrop(null)}
          dangerouslySetInnerHTML={{ __html: cropOffIconSvg }}
        />
        <span className="exp-ann-sep" />
        <button
          type="button"
          className="exp-ann-cancel"
          onClick={props.onCancel}
        >
          Cancel
        </button>
        <button
          type="button"
          className="exp-ann-save"
          onClick={() => props.onSave(shapesRef.current, cropRef.current)}
        >
          Done
        </button>
      </div>

      <div className="exp-ann-stage" ref={stageRef}>
        {image && display ? (
          <canvas
            ref={canvasRef}
            width={image.naturalWidth}
            height={image.naturalHeight}
            style={{ width: `${display.width}px`, height: `${display.height}px` }}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={commitDraft}
            onPointerCancel={onPointerCancel}
          />
        ) : (
          <div className="exp-spinner" />
        )}
      </div>
    </div>
  )
}
