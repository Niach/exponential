// Bake annotations into the screenshot: the server and every issue view see
// one plain image, no annotation sidecar format to sync.
import { encodeScreenshot } from "../capture/image"
import { drawShapes } from "./draw"
import { type AnnotationShape, strokeWidthFor } from "./shapes"

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

// Returns the re-encoded annotated image, the untouched base blob when there
// is nothing to draw, or null when decoding/encoding fails (caller keeps the
// unannotated screenshot — never blocks submission).
export async function flattenAnnotations(
  baseBlob: Blob,
  shapes: readonly AnnotationShape[]
): Promise<Blob | null> {
  if (shapes.length === 0) return baseBlob
  try {
    const image = await loadImage(baseBlob)
    const canvas = document.createElement(`canvas`)
    canvas.width = image.naturalWidth
    canvas.height = image.naturalHeight
    const context = canvas.getContext(`2d`)
    if (!context) return null
    context.drawImage(image, 0, 0)
    drawShapes(
      context,
      shapes,
      strokeWidthFor(canvas.width, canvas.height)
    )
    return await encodeScreenshot(canvas)
  } catch {
    return null
  }
}
