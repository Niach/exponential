// Bake annotations into the screenshot: the server and every issue view see
// one plain image, no annotation sidecar format to sync.
import { encodeScreenshot } from "../capture/image"
import { drawShapes } from "./draw"
import {
  type AnnotationShape,
  type NormalizedRect,
  resolveCrop,
  strokeWidthFor,
} from "./shapes"

export function loadImage(blob: Blob): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(blob)
    const image = new Image()
    image.onload = () => {
      URL.revokeObjectURL(url)
      resolve(image)
    }
    image.onerror = () => {
      URL.revokeObjectURL(url)
      reject(new Error(`screenshot decode failed`))
    }
    image.src = url
  })
}

// Returns the re-encoded annotated/cropped image, the untouched base blob when
// there is nothing to draw or crop, or null when decoding/encoding fails
// (caller keeps the unannotated screenshot — never blocks submission).
//
// The crop is applied FIRST by sizing the output canvas to the crop box and
// translating the drawing origin, so shapes (kept in original-image space)
// land correctly offset relative to the crop and stroke widths match the
// editor exactly (both derive from the original image dimensions).
export async function flattenAnnotations(
  baseBlob: Blob,
  shapes: readonly AnnotationShape[],
  crop: NormalizedRect | null = null
): Promise<Blob | null> {
  if (shapes.length === 0 && !crop) return baseBlob
  try {
    const image = await loadImage(baseBlob)
    const box = resolveCrop(crop, image.naturalWidth, image.naturalHeight)
    const canvas = document.createElement(`canvas`)
    canvas.width = box.width
    canvas.height = box.height
    const context = canvas.getContext(`2d`)
    if (!context) return null
    context.translate(-box.x, -box.y)
    context.drawImage(image, 0, 0)
    drawShapes(
      context,
      shapes,
      strokeWidthFor(image.naturalWidth, image.naturalHeight)
    )
    return await encodeScreenshot(canvas)
  } catch {
    return null
  }
}
