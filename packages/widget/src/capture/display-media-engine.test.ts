// EXP-435 native display capture: feature detection and the one hard
// guarantee — the media tracks stop no matter how the capture ends (the
// browser's "sharing your screen" indicator must never outlive the shot).
// happy-dom has no real video pipeline, so the success path (drawImage of a
// frame) is covered by the manual demo recipe instead.
import { afterEach, describe, expect, it, vi } from "vitest"
import {
  displayMediaEngine,
  isDisplayCaptureSupported,
} from "./display-media-engine"

const captureOpts = {
  excludeSelectors: [],
  keepNode: () => true,
  dpr: 1,
  beforeFrame: () => Promise.resolve(),
}

function fakeStream() {
  const stop = vi.fn()
  return {
    stream: { getTracks: () => [{ stop }, { stop }] },
    stop,
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
  delete (
    HTMLVideoElement.prototype as { requestVideoFrameCallback?: unknown }
  ).requestVideoFrameCallback
  delete (HTMLVideoElement.prototype as { srcObject?: unknown }).srcObject
})

// happy-dom's HTMLMediaElement rejects anything but a real MediaStream as
// srcObject; shadow the accessor so a fake stream gets past the grant.
function acceptFakeSrcObject() {
  Object.defineProperty(HTMLVideoElement.prototype, `srcObject`, {
    configurable: true,
    get: () => null,
    set: () => undefined,
  })
}

describe(`isDisplayCaptureSupported`, () => {
  it(`true only when getDisplayMedia exists`, () => {
    vi.stubGlobal(`navigator`, {
      mediaDevices: { getDisplayMedia: () => Promise.resolve() },
    })
    expect(isDisplayCaptureSupported()).toBe(true)
    vi.stubGlobal(`navigator`, { mediaDevices: {} })
    expect(isDisplayCaptureSupported()).toBe(false)
    vi.stubGlobal(`navigator`, {})
    expect(isDisplayCaptureSupported()).toBe(false)
  })
})

describe(`displayMediaEngine`, () => {
  it(`declares the precropped + long-timeout contract`, () => {
    // The frame is already the picked surface (no document crop), and the
    // interactive picker needs far more than the 6s DOM-raster budget.
    expect(displayMediaEngine.precropped).toBe(true)
    expect(displayMediaEngine.captureTimeoutMs).toBeGreaterThanOrEqual(30_000)
  })

  it(`stops every track when the capture fails after the grant`, async () => {
    const { stream, stop } = fakeStream()
    vi.stubGlobal(`navigator`, {
      mediaDevices: { getDisplayMedia: vi.fn(async () => stream) },
    })
    // Resolve the frame wait immediately; the happy-dom canvas then yields
    // no 2d context / zero dimensions, which must throw AND stop the tracks.
    ;(
      HTMLVideoElement.prototype as {
        requestVideoFrameCallback?: (callback: () => void) => void
      }
    ).requestVideoFrameCallback = (callback) => callback()
    HTMLVideoElement.prototype.play = vi.fn(async () => undefined)

    await expect(displayMediaEngine.capture(captureOpts)).rejects.toThrow()
    expect(stop).toHaveBeenCalledTimes(2)
  })

  // FEED-18: the delayed-capture hold runs AFTER the grant with the stream
  // still live (the picker consumed the click's activation, so it cannot run
  // before getDisplayMedia); the tracks stop only once the hold is over.
  it(`awaits beforeFrame after the grant, with the stream live`, async () => {
    const { stream, stop } = fakeStream()
    const getDisplayMedia = vi.fn(async () => stream)
    vi.stubGlobal(`navigator`, { mediaDevices: { getDisplayMedia } })
    acceptFakeSrcObject()
    ;(
      HTMLVideoElement.prototype as {
        requestVideoFrameCallback?: (callback: () => void) => void
      }
    ).requestVideoFrameCallback = (callback) => callback()
    HTMLVideoElement.prototype.play = vi.fn(async () => undefined)

    let releaseHold: () => void = () => {}
    const beforeFrame = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          releaseHold = resolve
        })
    )
    const result = displayMediaEngine.capture({ ...captureOpts, beforeFrame })
    await Promise.resolve()
    await Promise.resolve()
    await Promise.resolve()
    expect(getDisplayMedia).toHaveBeenCalledTimes(1)
    expect(beforeFrame).toHaveBeenCalledTimes(1)
    expect(stop).not.toHaveBeenCalled()

    releaseHold()
    // happy-dom yields no frame, so the grab throws — after the hold.
    await expect(result).rejects.toThrow()
    expect(stop).toHaveBeenCalledTimes(2)
  })

  it(`propagates a denied picker without touching tracks`, async () => {
    const getDisplayMedia = vi.fn(async () => {
      throw new DOMException(`Permission denied`, `NotAllowedError`)
    })
    vi.stubGlobal(`navigator`, { mediaDevices: { getDisplayMedia } })
    await expect(displayMediaEngine.capture(captureOpts)).rejects.toThrow(
      `Permission denied`
    )
  })
})
