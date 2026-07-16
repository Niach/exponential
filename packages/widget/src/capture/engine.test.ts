import { describe, expect, it, vi } from "vitest"
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
    await captureScreenshot(engine)

    expect(capture).toHaveBeenCalledTimes(1)
    const opts = capture.mock.calls[0][0]
    expect(opts.excludeSelectors).toContain(`[data-exponential-widget]`)
    expect(opts.keepNode(document.body)).toBe(true)

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
