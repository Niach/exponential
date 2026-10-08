// VAPP-87: the overlay placement rule replays its fixture byte for byte
// (the React renderer checks Radix against it in Chromium, the Rust core
// reproduces it).

import { describe, expect, test } from "bun:test"
import { OVERLAY_OFFSET, OVERLAY_PADDING, placeOverlay } from "./overlay"
import fixture from "../fixtures/overlay-geometry.json" with { type: "json" }
import type { OverlayPlacement, OverlaySide } from "./overlay"

describe(`overlay-geometry.json`, () => {
  test(`constants`, () => {
    expect(fixture.offset).toBe(OVERLAY_OFFSET)
    expect(fixture.padding).toBe(OVERLAY_PADDING)
  })
  test(`every case places as recorded`, () => {
    expect(fixture.cases.length).toBeGreaterThan(8)
    for (const c of fixture.cases) {
      expect(placeOverlay(c.anchor, c.size, c.viewport, { side: c.side as OverlaySide }), c.name).toEqual(c.expected as OverlayPlacement)
    }
  })
  test(`a flip happens only when the opposite side has MORE room`, () => {
    const tight = placeOverlay({ x: 0, y: 290, width: 100, height: 20 }, { width: 100, height: 400 }, { width: 400, height: 600 }, { side: `bottom` })
    expect(tight.flipped).toBe(false)
    expect(tight.side).toBe(`bottom`)
  })
})
