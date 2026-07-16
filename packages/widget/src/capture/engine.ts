import {
  cropToViewport,
  encodeScreenshot,
} from "./image"
import { isReadableIframe } from "./pii-mask"

// Engine abstraction: snapDOM today, a marker.io-style server-side renderer
// or native tab capture can slot in later without touching the UI.
export interface CaptureEngine {
  readonly name: string
  capture(opts: {
    excludeSelectors: string[]
    keepNode(el: Element): boolean
    dpr: number
  }): Promise<HTMLCanvasElement>
}

const captureTimeoutMs = 6_000
const maxOutputEdge = 1920

function findTaintedCanvases(): Set<Element> {
  const tainted = new Set<Element>()
  for (const canvas of Array.from(document.querySelectorAll(`canvas`))) {
    try {
      // Probing with a 1x1 read is enough to trip the tainted-canvas check.
      canvas.toDataURL()
    } catch {
      tainted.add(canvas)
    }
  }
  return tainted
}

function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`capture timed out`)),
      ms
    )
    promise.then(
      (value) => {
        clearTimeout(timer)
        resolve(value)
      },
      (error: unknown) => {
        clearTimeout(timer)
        reject(error instanceof Error ? error : new Error(String(error)))
      }
    )
  })
}

// The single entry point the UI calls. NEVER throws: any failure (tainted
// content, timeout, encoder trouble) resolves null and the form simply
// proceeds without a screenshot.
export async function captureScreenshot(
  engine: CaptureEngine
): Promise<Blob | null> {
  try {
    const tainted = findTaintedCanvases()
    const bodyWidth = document.body.getBoundingClientRect().width

    const canvas = await withTimeout(
      engine.capture({
        // The widget excludes itself from the clone — capture runs before
        // the panel opens, so at most the floating button is at stake.
        excludeSelectors: [`[data-exponential-widget]`],
        // Readable iframes are dropped whole: snapDOM rasterizes them before
        // the pii-mask plugin can walk their text (see isReadableIframe).
        keepNode: (el) => !tainted.has(el) && !isReadableIframe(el),
        dpr: Math.min(window.devicePixelRatio || 1, 2),
      }),
      captureTimeoutMs
    )

    const cropped = cropToViewport(canvas, {
      sourceCssWidth: bodyWidth,
      scrollX: window.scrollX,
      scrollY: window.scrollY,
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
      maxEdge: maxOutputEdge,
    })

    return await encodeScreenshot(cropped)
  } catch {
    return null
  }
}
