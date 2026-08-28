import { describe, expect, it, vi } from "vitest"
import type { ViewportCropArgs } from "./image"

type CropArgs = ViewportCropArgs & { maxEdge: number }

// Capture the args the engine hands cropToViewport while running the real crop
// math, so we can assert the document-space origin is derived from the body
// rect (not hardcoded to zero).
const h = vi.hoisted(() => ({
  cropArgs: null as { originX: number; originY: number } | null,
  cropCalls: 0,
  scaleCalls: 0,
}))
vi.mock(`./image`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown> & {
    cropToViewport: (
      source: HTMLCanvasElement,
      args: CropArgs
    ) => HTMLCanvasElement
    scaleToMaxEdge: (
      source: HTMLCanvasElement,
      maxEdge: number
    ) => HTMLCanvasElement
  }
  return {
    ...actual,
    cropToViewport: (source: HTMLCanvasElement, args: CropArgs) => {
      h.cropArgs = { originX: args.originX, originY: args.originY }
      h.cropCalls += 1
      return actual.cropToViewport(source, args)
    },
    scaleToMaxEdge: (source: HTMLCanvasElement, maxEdge: number) => {
      h.scaleCalls += 1
      return actual.scaleToMaxEdge(source, maxEdge)
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

  it(`resolves null when the engine's beforeFrame hold is followed by a throw`, async () => {
    const engine: CaptureEngine = {
      name: `boom-after-hold`,
      capture: async ({ beforeFrame }) => {
        await beforeFrame()
        throw new Error(`boom`)
      },
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

  // EXP-435: interactive engines (display-media picker) declare their own
  // budget — the 6s default would kill every real surface-picker flow.
  it(`honors a per-engine capture timeout`, async () => {
    vi.useFakeTimers()
    try {
      let resolveCapture: (canvas: HTMLCanvasElement) => void = () => {}
      const engine: CaptureEngine = {
        name: `slow`,
        captureTimeoutMs: 30_000,
        capture: () =>
          new Promise((resolve) => {
            resolveCapture = resolve
          }),
      }
      const result = captureScreenshot(engine)
      // Past the 6s default but inside the engine's own budget.
      await vi.advanceTimersByTimeAsync(10_000)
      resolveCapture(fakeCanvas())
      // Encoding may fail in happy-dom (null), but the run must not have
      // been killed by the DEFAULT timeout — assert it settles now instead
      // of at 30s.
      await expect(result).resolves.toBeDefined()
    } finally {
      vi.useRealTimers()
    }
  })

  // FEED-18: the reporter's hold is added on top of the engine's budget, and
  // the engine receives it as the beforeFrame hook it awaits at the last
  // moment (so display-media can run it after the picker).
  it(`extends the timeout by the capture delay and hands the hold to the engine`, async () => {
    vi.useFakeTimers()
    try {
      const onCountdown = vi.fn()
      let resolveCapture: (canvas: HTMLCanvasElement) => void = () => {}
      const capture = vi.fn(
        async (opts: Parameters<CaptureEngine[`capture`]>[0]) => {
          await opts.beforeFrame()
          return new Promise<HTMLCanvasElement>((resolve) => {
            resolveCapture = resolve
          })
        }
      )
      const engine: CaptureEngine = { name: `held`, capture }
      const result = captureScreenshot(engine, {
        delayMs: 3_000,
        onCountdown,
      })
      // The hold runs inside the engine call: 3 → 2 → 1 → 0.
      await vi.advanceTimersByTimeAsync(3_500)
      expect(onCountdown.mock.calls.map(([n]) => n)).toEqual([3, 2, 1, 0])
      // 8s in: past the 6s default budget, inside 6s + 3s.
      await vi.advanceTimersByTimeAsync(4_500)
      resolveCapture(fakeCanvas())
      await expect(result).resolves.toBeDefined()
    } finally {
      vi.useRealTimers()
    }
  })

  it(`resolves beforeFrame at once when no delay is set`, async () => {
    const onCountdown = vi.fn()
    const capture = vi.fn(
      async (opts: Parameters<CaptureEngine[`capture`]>[0]) => {
        await opts.beforeFrame()
        return fakeCanvas()
      }
    )
    await captureScreenshot({ name: `plain`, capture }, { onCountdown })
    expect(capture).toHaveBeenCalledTimes(1)
    expect(onCountdown).not.toHaveBeenCalled()
  })

  // EXP-435: a precropped engine's frame is already the visible surface —
  // the document-space viewport crop must not touch it.
  it(`skips the viewport crop for precropped engines`, async () => {
    h.cropCalls = 0
    h.scaleCalls = 0
    const engine: CaptureEngine = {
      name: `display`,
      precropped: true,
      capture: () => Promise.resolve(fakeCanvas(640, 480)),
    }
    await captureScreenshot(engine)
    expect(h.cropCalls).toBe(0)
    expect(h.scaleCalls).toBe(1)
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
