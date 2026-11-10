// VAPP-87: the overlay placement rule replays its fixture byte for byte
// (the React renderer checks Radix against it in Chromium, the Rust core
// reproduces it).

import { describe, expect, test } from "bun:test"
import { OVERLAY_OFFSET, OVERLAY_PADDING, anchorPosition, placeInsetOverlay, placeOverlay } from "./overlay"
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
  test(`matchAnchorWidth: the panel is the anchor's width (a trigger and an anchor by id), painted from a narrower content box`, () => {
    const matched = fixture.cases.filter((c) => (c as { matchAnchorWidth?: boolean }).matchAnchorWidth) as { anchor: { width: number }; size: { width: number }; contentWidth: number; byId?: boolean }[]
    expect(matched.some((c) => c.byId)).toBe(true)
    expect(matched.some((c) => !c.byId)).toBe(true)
    for (const c of matched) {
      expect(c.size.width).toBe(c.anchor.width)
      expect(c.contentWidth).toBeLessThan(c.anchor.width)
    }
  })
  test(`VAPP-281: a hosted popup sits against its anchor, no flip, no shift (outside the surface)`, () => {
    expect(fixture.hosted.length).toBeGreaterThan(4)
    for (const c of fixture.hosted) {
      expect(anchorPosition(c.anchor, c.size, c.side as OverlaySide, c.align as OverlayAlign, fixture.offset), c.name).toEqual(c.expected)
    }
    expect(fixture.hosted.some((c) => c.expected.y < 0)).toBe(true)
    expect(fixture.hosted.some((c) => c.expected.x < 0)).toBe(true)
    // In-surface the same popup shifts inside; hosted it does not.
    const c = fixture.hosted.find((h) => h.expected.x < 0 && h.align === `end`)!
    expect(placeOverlay(c.anchor, c.size, { width: c.surface.width, height: 600 }, { side: c.side as OverlaySide, align: `end` }).x).toBe(fixture.padding)
  })
  test(`#EXP-1765: an inset popup takes the inset, then the clamp, then whole px from its anchor`, () => {
    expect(fixture.inset.length).toBeGreaterThanOrEqual(5)
    for (const c of fixture.inset) {
      expect(placeInsetOverlay(c.anchor, c.size, c.viewport, { side: c.side as OverlaySide, align: c.align as OverlayAlign, offset: c.offset, padding: c.padding }, c.inset), c.name).toEqual(c.expected as OverlayPlacement)
    }
    // The snap rounds half away from zero (gpui's): -59.5 px lands on -60.
    expect(fixture.inset.some((c) => c.expected.x % 1 !== 0 && c.anchor.x % 1 !== 0)).toBe(true)
  })
  test(`a flip happens only when the opposite side has MORE room`, () => {
    const tight = placeOverlay({ x: 0, y: 290, width: 100, height: 20 }, { width: 100, height: 400 }, { width: 400, height: 600 }, { side: `bottom` })
    expect(tight.flipped).toBe(false)
    expect(tight.side).toBe(`bottom`)
  })
})
