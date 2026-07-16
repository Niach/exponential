// Pure-ish image math + encoding helpers, split out for unit tests.

export interface Dimensions {
  width: number
  height: number
}

// Scale (down only) so the longest edge fits maxEdge, preserving aspect.
export function fitWithin(
  width: number,
  height: number,
  maxEdge: number
): Dimensions {
  if (width <= 0 || height <= 0) return { width: 0, height: 0 }
  const longest = Math.max(width, height)
  if (longest <= maxEdge) return { width, height }
  const scale = maxEdge / longest
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  }
}

export function canvasToBlob(
  canvas: HTMLCanvasElement,
  type: string,
  quality?: number
): Promise<Blob | null> {
  return new Promise((resolve) => {
    try {
      // Per spec, unsupported types silently fall back to PNG — callers
      // check blob.type rather than trusting the request.
      canvas.toBlob((blob) => resolve(blob), type, quality)
    } catch {
      resolve(null)
    }
  })
}

const preferredMaxBytes = 5 * 1024 * 1024
// Stay under the server's 10MB attachment cap with multipart headroom.
const hardMaxBytes = 9 * 1024 * 1024

// WebP 0.9 → (Safari falls back to PNG automatically) → JPEG 0.8 if still
// heavy → null if even that exceeds the hard cap (caller drops the shot).
export async function encodeScreenshot(
  canvas: HTMLCanvasElement
): Promise<Blob | null> {
  const first = await canvasToBlob(canvas, `image/webp`, 0.9)
  if (first && first.size <= preferredMaxBytes) return first

  const jpeg = await canvasToBlob(canvas, `image/jpeg`, 0.8)
  const best = [first, jpeg]
    .filter((blob): blob is Blob => blob !== null)
    .sort((a, b) => a.size - b.size)[0]
  if (!best) return null
  return best.size <= hardMaxBytes ? best : null
}

export function screenshotFilename(blob: Blob): string {
  if (blob.type === `image/webp`) return `screenshot.webp`
  if (blob.type === `image/jpeg`) return `screenshot.jpg`
  return `screenshot.png`
}

export interface ViewportCropArgs {
  sourceCssWidth: number
  // Document-space origin of the captured element (its border-box top-left).
  // snapDOM rasterizes document.body, whose origin is offset by any body
  // margin — the viewport's document origin is (scrollX, scrollY), so the crop
  // starts at (scroll − origin) × factor. Passing origin explicitly (rather
  // than assuming zero) keeps the crop correct for any body margin and for a
  // future non-body capture root.
  originX: number
  originY: number
  scrollX: number
  scrollY: number
  viewportWidth: number
  viewportHeight: number
}

// Pure viewport-crop geometry in the source canvas's pixel space, split out so
// it can be unit-tested (happy-dom's canvas 2d context is null, so any
// canvas-level test silently hits cropToViewport's uncropped fallback).
// Returns null when the crop degenerates (scrolled past the captured content).
export function viewportCropRect(
  source: Dimensions,
  args: ViewportCropArgs
): { x: number; y: number; width: number; height: number } | null {
  const factor = args.sourceCssWidth > 0 ? source.width / args.sourceCssWidth : 1
  const x = Math.max(0, Math.floor((args.scrollX - args.originX) * factor))
  const y = Math.max(0, Math.floor((args.scrollY - args.originY) * factor))
  const width = Math.min(source.width - x, Math.ceil(args.viewportWidth * factor))
  const height = Math.min(
    source.height - y,
    Math.ceil(args.viewportHeight * factor)
  )
  if (width <= 0 || height <= 0) return null
  return { x, y, width, height }
}

// Crop the full-page capture down to the visible viewport, then downscale to
// the target edge. `sourceCssWidth` is the CSS-pixel width of the captured
// element so we can derive the raster scale factor; `originX`/`originY` are its
// document-space origin (see ViewportCropArgs).
export function cropToViewport(
  source: HTMLCanvasElement,
  args: ViewportCropArgs & { maxEdge: number }
): HTMLCanvasElement {
  const factor =
    args.sourceCssWidth > 0 ? source.width / args.sourceCssWidth : 1
  const rect = viewportCropRect(source, args)
  if (!rect) return source

  const target = fitWithin(rect.width, rect.height, args.maxEdge * factor)
  const scaled = fitWithin(target.width, target.height, args.maxEdge)

  const output = document.createElement(`canvas`)
  output.width = scaled.width
  output.height = scaled.height
  const context = output.getContext(`2d`)
  if (!context) return source
  context.drawImage(
    source,
    rect.x,
    rect.y,
    rect.width,
    rect.height,
    0,
    0,
    scaled.width,
    scaled.height
  )
  return output
}
