import { describe, expect, it } from "vitest"
import { fitWithin, screenshotFilename } from "./image"

describe(`fitWithin`, () => {
  it(`keeps dimensions under the max edge`, () => {
    expect(fitWithin(800, 600, 1920)).toEqual({ width: 800, height: 600 })
  })

  it(`scales down landscape preserving aspect`, () => {
    expect(fitWithin(3840, 2160, 1920)).toEqual({ width: 1920, height: 1080 })
  })

  it(`scales down portrait preserving aspect`, () => {
    expect(fitWithin(1000, 4000, 2000)).toEqual({ width: 500, height: 2000 })
  })

  it(`never returns zero for tiny aspect ratios`, () => {
    expect(fitWithin(10_000, 1, 100).height).toBeGreaterThanOrEqual(1)
  })

  it(`handles degenerate input`, () => {
    expect(fitWithin(0, 100, 500)).toEqual({ width: 0, height: 0 })
  })
})

describe(`screenshotFilename`, () => {
  it(`derives the extension from the blob type`, () => {
    expect(screenshotFilename(new Blob([], { type: `image/webp` }))).toBe(
      `screenshot.webp`
    )
    expect(screenshotFilename(new Blob([], { type: `image/jpeg` }))).toBe(
      `screenshot.jpg`
    )
    expect(screenshotFilename(new Blob([], { type: `image/png` }))).toBe(
      `screenshot.png`
    )
  })
})
