// FEED-200: the web pager's paint (tabs-swipe.ts): where a panel and the
// header thumb are drawn at rest and under a finger, RTL mirrored.

import { describe, expect, it } from "bun:test"
import { tabPage, tabPageStyle, tabThumbStyle, sameSpans } from "./tabs-swipe"

const spans = [
  { x: 4, y: 2, w: 60, h: 28 },
  { x: 64, y: 2, w: 80, h: 28 },
  { x: 144, y: 2, w: 40, h: 28 },
]

describe(`tabs-swipe paint`, () => {
  it(`only the active panel and its two neighbours are drawn`, () => {
    expect([0, 1, 2, 3].map((i) => tabPage(i, 1))).toEqual([-1, 0, 1, null])
  })
  it(`panels sit one width to their side, the next on the left under RTL`, () => {
    expect(tabPageStyle(0, null, false).transform).toBe(`none`)
    expect(tabPageStyle(1, null, false).transform).toBe(`translateX(100%)`)
    expect(tabPageStyle(1, null, true).transform).toBe(`translateX(-100%)`)
    expect(tabPageStyle(-1, { offset: -120, width: 400 }, false)).toEqual({ transform: `translateX(calc(-100% + -120px))`, transition: `none` })
    expect(tabPageStyle(0, { offset: 30, width: 400 }, false).transform).toBe(`translateX(30px)`)
    expect(tabPageStyle(0, null, false).transition).toContain(`var(--xui-motion-standard)`)
  })
  it(`the thumb sits on the active header, carried towards the neighbour by the drag`, () => {
    expect(tabThumbStyle(spans, 1, null, false)).toMatchObject({ transform: `translateX(64px)`, width: `80px`, top: `2px`, height: `28px` })
    // Half a page left = half way to the next header.
    expect(tabThumbStyle(spans, 1, { offset: -200, width: 400 }, false)).toMatchObject({ transform: `translateX(104px)`, width: `60px`, transition: `none` })
    // RTL: dragging right reveals the next one.
    expect(tabThumbStyle(spans, 1, { offset: 200, width: 400 }, true)).toMatchObject({ transform: `translateX(104px)`, width: `60px` })
    // Past the end: no neighbour, the thumb stays.
    expect(tabThumbStyle(spans, 2, { offset: -100, width: 400 }, false)).toMatchObject({ transform: `translateX(144px)`, width: `40px` })
    expect(tabThumbStyle([null], 0, null, false)).toBeNull()
  })
  it(`spans compare within half a pixel`, () => {
    expect(sameSpans(spans, spans.map((s) => ({ ...s, w: s.w + 1 })))).toBe(false)
    expect(sameSpans(null, spans)).toBe(false)
  })
})
