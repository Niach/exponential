import { describe, expect, it } from "vitest"
import { fitWithin, screenshotFilename, viewportCropRect } from "./image"

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

// A 2528×4000 raster of a 1264 CSS-px-wide body → raster factor 2.
const source = { width: 2528, height: 4000 }

describe(`viewportCropRect`, () => {
  it(`crops at the scroll position when the capture origin is zero`, () => {
    // Baseline: the pre-origin behavior for a margin-less body must be
    // unchanged (origin 0,0 collapses to scroll × factor).
    expect(
      viewportCropRect(source, {
        sourceCssWidth: 1264,
        originX: 0,
        originY: 0,
        scrollX: 0,
        scrollY: 500,
        viewportWidth: 1000,
        viewportHeight: 800,
      })
    ).toEqual({ x: 0, y: 1000, width: 2000, height: 1600 })
  })

  it(`shifts the crop up by a body margin on a scrolled page`, () => {
    // An 8px body margin lifts the raster origin, so the viewport sits 8 CSS
    // px (16 raster px) higher than the naive scroll×factor position.
    expect(
      viewportCropRect(source, {
        sourceCssWidth: 1264,
        originX: 8,
        originY: 8,
        scrollX: 0,
        scrollY: 500,
        viewportWidth: 1000,
        viewportHeight: 800,
      })
    ).toEqual({ x: 0, y: 984, width: 2000, height: 1600 })
  })

  it(`clamps a margin offset to zero on an unscrolled page`, () => {
    expect(
      viewportCropRect(source, {
        sourceCssWidth: 1264,
        originX: 8,
        originY: 8,
        scrollX: 0,
        scrollY: 0,
        viewportWidth: 1000,
        viewportHeight: 800,
      })
    ).toEqual({ x: 0, y: 0, width: 2000, height: 1600 })
  })

  it(`clamps width/height to the canvas remaining past the crop origin`, () => {
    expect(
      viewportCropRect(source, {
        sourceCssWidth: 1264,
        originX: 0,
        originY: 0,
        scrollX: 700,
        scrollY: 1900,
        viewportWidth: 1000,
        viewportHeight: 800,
      })
    ).toEqual({ x: 1400, y: 3800, width: 1128, height: 200 })
  })

  it(`returns null when scrolled past the captured content`, () => {
    expect(
      viewportCropRect(source, {
        sourceCssWidth: 1264,
        originX: 0,
        originY: 0,
        scrollX: 0,
        scrollY: 2100,
        viewportWidth: 1000,
        viewportHeight: 800,
      })
    ).toBeNull()
  })

  it(`falls back to factor 1 when the source CSS width is zero`, () => {
    expect(
      viewportCropRect(
        { width: 1000, height: 800 },
        {
          sourceCssWidth: 0,
          originX: 0,
          originY: 0,
          scrollX: 0,
          scrollY: 100,
          viewportWidth: 500,
          viewportHeight: 400,
        }
      )
    ).toEqual({ x: 0, y: 100, width: 500, height: 400 })
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
