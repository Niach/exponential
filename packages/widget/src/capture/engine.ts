import {
  cropToViewport,
  encodeScreenshot,
  scaleToMaxEdge,
} from "./image"
import { isReadableIframe } from "./pii-mask"
import { runCountdown } from "./countdown"

// Engine abstraction: snapDOM and native display capture today, a marker.io
// style server-side renderer can slot in later without touching the UI.
export interface CaptureEngine {
  readonly name: string
  // True when the engine's output is already the visible surface (a
  // display-media frame): captureScreenshot skips the document→viewport crop
  // and only applies the maxEdge downscale.
  readonly precropped?: boolean
  // Interactive engines (surface picker) need far longer than the 6s
  // default a DOM raster gets.
  readonly captureTimeoutMs?: number
  capture(opts: CaptureOptions): Promise<HTMLCanvasElement>
}

export interface CaptureOptions {
  excludeSelectors: string[]
  keepNode(el: Element): boolean
  dpr: number
  // Awaited at the last moment before the frame is taken — after any
  // interactive step (the display-media picker) so a delayed capture
  // (FEED-18) never outlives the click's transient user activation.
  beforeFrame(): Promise<void>
}

export interface CaptureScreenshotOptions {
  // Hold the frame this long so the reporter can open a menu/popup first.
  delayMs?: number
  // Whole seconds left, once per second, then 0 right before the frame.
  onCountdown?(secondsLeft: number): void
}

const defaultCaptureTimeoutMs = 6_000
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
  engine: CaptureEngine,
  options: CaptureScreenshotOptions = {}
): Promise<Blob | null> {
  try {
    const tainted = findTaintedCanvases()
    const bodyRect = document.body.getBoundingClientRect()
    const delayMs = Math.max(0, options.delayMs ?? 0)

    const canvas = await withTimeout(
      engine.capture({
        // The widget excludes itself from the clone — capture runs before
        // the panel opens, so at most the floating button is at stake.
        excludeSelectors: [`[data-exponential-widget]`],
        // Readable iframes are dropped whole: snapDOM rasterizes them before
        // the pii-mask plugin can walk their text (see isReadableIframe).
        keepNode: (el) => !tainted.has(el) && !isReadableIframe(el),
        dpr: Math.min(window.devicePixelRatio || 1, 2),
        beforeFrame: () => runCountdown(delayMs, options.onCountdown),
      }),
      // The hold is the reporter's own time, not capture work — it must
      // never eat into the engine's budget.
      (engine.captureTimeoutMs ?? defaultCaptureTimeoutMs) + delayMs
    )

    // A precropped engine's frame IS the visible surface — cropping it with
    // document geometry would cut real content; only the size cap applies.
    const cropped = engine.precropped
      ? scaleToMaxEdge(canvas, maxOutputEdge)
      : cropToViewport(canvas, {
          sourceCssWidth: bodyRect.width,
          // getBoundingClientRect is viewport-relative; adding scroll lifts
          // it to the document-space origin snapDOM rasterizes from.
          originX: bodyRect.left + window.scrollX,
          originY: bodyRect.top + window.scrollY,
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
