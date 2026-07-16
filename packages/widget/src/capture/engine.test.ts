import { describe, expect, it, vi } from "vitest"
import type { ViewportCropArgs } from "./image"

type CropArgs = ViewportCropArgs & { maxEdge: number }

// Capture the args the engine hands cropToViewport while running the real crop
// math, so we can assert the document-space origin is derived from the body
// rect (not hardcoded to zero).
const h = vi.hoisted(() => ({
  cropArgs: null as { originX: number; originY: number } | null,
}))
vi.mock(`./image`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown> & {
    cropToViewport: (
      source: HTMLCanvasElement,
      args: CropArgs
    ) => HTMLCanvasElement
  }
  return {
    ...actual,
    cropToViewport: (source: HTMLCanvasElement, args: CropArgs) => {
      h.cropArgs = { originX: args.originX, originY: args.originY }
      return actual.cropToViewport(source, args)
    },
  }
})

import { captureScreenshot, type CaptureEngine } from "./engine"

function fakeCanvas(width = 100, height = 80): HTMLCanvasElement {
  const canvas = document.createElement(`canvas`)
  canvas.width = width
  canvas.height = height
  return canvas
}

describe(`captureScreenshot`, () => {
  it(`resolves null when the engine throws`, async () => {
    const engine: CaptureEngine = {
      name: `boom`,
      capture: () => Promise.reject(new Error(`boom`)),
    }
    await expect(captureScreenshot(engine)).resolves.toBeNull()
  })

  it(`resolves null when the engine hangs past the timeout`, async () => {
    vi.useFakeTimers()
    try {
      const engine: CaptureEngine = {
        name: `hang`,
        capture: () => new Promise(() => {}),
      }
      const result = captureScreenshot(engine)
      await vi.advanceTimersByTimeAsync(7_000)
      await expect(result).resolves.toBeNull()
    } finally {
      vi.useRealTimers()
    }
  })

  it(`passes the widget exclusion selector and a keep predicate`, async () => {
    const capture = vi.fn(
      (_opts: Parameters<CaptureEngine[`capture`]>[0]) =>
        Promise.resolve(fakeCanvas())
    )
    const engine: CaptureEngine = { name: `spy`, capture }
    // A non-zero body rect stands in for a page with a body margin; scrollX/Y
    // are 0 in happy-dom, so the crop origin must equal the rect offset.
    vi.spyOn(document.body, `getBoundingClientRect`).mockReturnValue({
      left: 3,
      top: 5,
      width: 400,
      height: 300,
      right: 403,
      bottom: 305,
      x: 3,
      y: 5,
      toJSON: () => ({}),
    } as DOMRect)
    await captureScreenshot(engine)

    expect(capture).toHaveBeenCalledTimes(1)
    const opts = capture.mock.calls[0][0]
    expect(opts.excludeSelectors).toContain(`[data-exponential-widget]`)
    expect(opts.keepNode(document.body)).toBe(true)
    expect(h.cropArgs).toEqual({ originX: 3, originY: 5 })

    // Same-origin iframes must be dropped: snapdom rasterizes them during
    // clone construction, before the pii-mask plugin can redact their text.
    const iframe = document.createElement(`iframe`)
    document.body.appendChild(iframe)
    try {
      expect(opts.keepNode(iframe)).toBe(false)
    } finally {
      iframe.remove()
    }
    expect(opts.dpr).toBeGreaterThan(0)
    expect(opts.dpr).toBeLessThanOrEqual(2)
  })
})
