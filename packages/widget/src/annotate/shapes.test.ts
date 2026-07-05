import { describe, expect, it } from "vitest"
import {
  type AnnotationShape,
  arrowHead,
  arrowHeadLength,
  clampPoint,
  distance,
  isDegenerate,
  normalizeRect,
  resolveCrop,
  strokeWidthFor,
} from "./shapes"

describe(`normalizeRect`, () => {
  it(`keeps an already-normalized pair`, () => {
    expect(normalizeRect({ x: 10, y: 20 }, { x: 110, y: 70 })).toEqual({
      x: 10,
      y: 20,
      width: 100,
      height: 50,
    })
  })

  it(`normalizes a drag in any direction`, () => {
    expect(normalizeRect({ x: 110, y: 70 }, { x: 10, y: 20 })).toEqual({
      x: 10,
      y: 20,
      width: 100,
      height: 50,
    })
    expect(normalizeRect({ x: 10, y: 70 }, { x: 110, y: 20 })).toEqual({
      x: 10,
      y: 20,
      width: 100,
      height: 50,
    })
  })
})

describe(`strokeWidthFor`, () => {
  it(`enforces the 3px minimum on small images`, () => {
    expect(strokeWidthFor(320, 240)).toBe(3)
  })

  it(`scales with the long edge`, () => {
    expect(strokeWidthFor(1920, 1080)).toBe(7)
    expect(strokeWidthFor(1080, 1920)).toBe(7)
  })

  it(`survives degenerate dimensions`, () => {
    expect(strokeWidthFor(0, 0)).toBe(3)
  })
})

describe(`arrowHead`, () => {
  const from = { x: 0, y: 0 }
  const to = { x: 100, y: 0 }
  const strokeWidth = 5
  const head = arrowHead(from, to, strokeWidth)
  const length = arrowHeadLength(strokeWidth)

  it(`puts the tip at the drag end`, () => {
    expect(head.tip).toEqual(to)
  })

  it(`places both wings one head-length from the tip`, () => {
    expect(distance(head.left, head.tip)).toBeCloseTo(length, 6)
    expect(distance(head.right, head.tip)).toBeCloseTo(length, 6)
  })

  it(`keeps the wings symmetric about the shaft`, () => {
    // Shaft lies on the x-axis: wings mirror in y, share x.
    expect(head.left.x).toBeCloseTo(head.right.x, 6)
    expect(head.left.y).toBeCloseTo(-head.right.y, 6)
    expect(head.left.y).not.toBeCloseTo(0, 1)
  })

  it(`pulls the shaft end back inside the head`, () => {
    expect(head.shaftEnd.y).toBeCloseTo(0, 6)
    expect(head.shaftEnd.x).toBeLessThan(to.x)
    expect(head.shaftEnd.x).toBeGreaterThan(to.x - length)
  })

  it(`scales the head with stroke width but never below 10px`, () => {
    expect(arrowHeadLength(1)).toBe(10)
    expect(arrowHeadLength(8)).toBe(32)
  })
})

describe(`isDegenerate`, () => {
  it(`drops click-without-drag rects and arrows`, () => {
    const rect: AnnotationShape = {
      tool: `rect`,
      points: [
        { x: 50, y: 50 },
        { x: 52, y: 51 },
      ],
    }
    expect(isDegenerate(rect)).toBe(true)
    expect(isDegenerate({ ...rect, tool: `arrow` })).toBe(true)
  })

  it(`keeps real drags`, () => {
    const arrow: AnnotationShape = {
      tool: `arrow`,
      points: [
        { x: 0, y: 0 },
        { x: 30, y: 10 },
      ],
    }
    expect(isDegenerate(arrow)).toBe(false)
  })

  it(`treats single-point shapes as degenerate`, () => {
    expect(
      isDegenerate({ tool: `rect`, points: [{ x: 1, y: 1 }] })
    ).toBe(true)
  })

  it(`keeps tiny pen scribbles (dot markers) but drops single samples`, () => {
    expect(
      isDegenerate({ tool: `pen`, points: [{ x: 5, y: 5 }] })
    ).toBe(true)
    expect(
      isDegenerate({
        tool: `pen`,
        points: [
          { x: 5, y: 5 },
          { x: 6, y: 5 },
        ],
      })
    ).toBe(false)
  })
})

describe(`resolveCrop`, () => {
  it(`returns the full image for a null crop`, () => {
    expect(resolveCrop(null, 800, 600)).toEqual({
      x: 0,
      y: 0,
      width: 800,
      height: 600,
    })
  })

  it(`keeps a crop that sits inside the image`, () => {
    expect(resolveCrop({ x: 100, y: 50, width: 300, height: 200 }, 800, 600))
      .toEqual({ x: 100, y: 50, width: 300, height: 200 })
  })

  it(`clamps a crop that overflows the image bounds`, () => {
    expect(resolveCrop({ x: 700, y: 500, width: 400, height: 400 }, 800, 600))
      .toEqual({ x: 700, y: 500, width: 100, height: 100 })
  })

  it(`falls back to the full image for a degenerate crop`, () => {
    expect(resolveCrop({ x: 10, y: 10, width: 0, height: 50 }, 800, 600))
      .toEqual({ x: 0, y: 0, width: 800, height: 600 })
    // Origin already past the right edge leaves zero width → full image.
    expect(resolveCrop({ x: 800, y: 0, width: 50, height: 50 }, 800, 600))
      .toEqual({ x: 0, y: 0, width: 800, height: 600 })
  })
})

describe(`clampPoint`, () => {
  it(`clamps to the image bounds`, () => {
    expect(clampPoint({ x: -4, y: 30 }, 100, 50)).toEqual({ x: 0, y: 30 })
    expect(clampPoint({ x: 120, y: 60 }, 100, 50)).toEqual({ x: 100, y: 50 })
    expect(clampPoint({ x: 40, y: 20 }, 100, 50)).toEqual({ x: 40, y: 20 })
  })
})
