// VAPP-87: the overlay placement rule replays its fixture byte for byte
// (the React renderer checks Radix against it in Chromium, the Rust core
// reproduces it).

import { describe, expect, test } from "bun:test"
import { OVERLAY_OFFSET, OVERLAY_PADDING, placeOverlay } from "./overlay"
import fixture from "../fixtures/overlay-geometry.json" with { type: "json" }
import type { OverlayAlign, OverlayPlacement, OverlaySide } from "./overlay"

describe(`overlay-geometry.json`, () => {
  test(`constants`, () => {
    expect(fixture.offset).toBe(OVERLAY_OFFSET)
    expect(fixture.padding).toBe(OVERLAY_PADDING)
  })
  test(`every case places as recorded`, () => {
    expect(fixture.cases.length).toBeGreaterThan(8)
    for (const c of fixture.cases) {
      expect(placeOverlay(c.anchor, c.size, c.viewport, { side: c.side as OverlaySide, align: ((c as { align?: string }).align ?? `center`) as OverlayAlign }), c.name).toEqual(c.expected as OverlayPlacement)
    }
  })
  test(`VAPP-276: every placement (side × start/end), the flip at the bottom edge, the shifts at both side edges and anchor-by-id cases are locked`, () => {
    const aligned = fixture.cases.filter((c) => `align` in c) as { side: string; align: string; byId?: boolean; expected: OverlayPlacement }[]
    const placements = new Set(aligned.map((c) => `${c.side}-${c.align}`))
    for (const p of [`top-start`, `top-end`, `bottom-start`, `bottom-end`, `right-start`, `left-end`]) expect(placements.has(p), p).toBe(true)
    expect(aligned.some((c) => c.side === `bottom` && c.expected.flipped)).toBe(true)
    expect(aligned.some((c) => c.expected.x === fixture.padding)).toBe(true)
    expect(aligned.some((c) => c.expected.x === 800 - fixture.padding - 240)).toBe(true)
    expect(aligned.filter((c) => c.byId).length).toBeGreaterThan(1)
  })
  test(`a flip happens only when the opposite side has MORE room`, () => {
    const tight = placeOverlay({ x: 0, y: 290, width: 100, height: 20 }, { width: 100, height: 400 }, { width: 400, height: 600 }, { side: `bottom` })
    expect(tight.flipped).toBe(false)
    expect(tight.side).toBe(`bottom`)
  })
})
