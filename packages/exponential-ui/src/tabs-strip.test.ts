import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/tabs-strip.json" with { type: "json" }
import { moveTab, partitionTabs, tabDropSlot, tabKeySlot, tabReorder } from "./tabs-strip"

describe(`VAPP-279 fixtures/tabs-strip.json`, () => {
  test(`partition`, () => {
    for (const c of fixture.partition) expect(partitionTabs(c.widths, c.available, c.gap, c.overflowW, c.active), c.name).toEqual(c.visible)
  })
  test(`dropSlot`, () => {
    for (const c of fixture.dropSlot) expect(tabDropSlot(c.centers, c.from, c.delta, c.rtl), c.name).toBe(c.slot)
  })
  test(`keySlot`, () => {
    for (const c of fixture.keySlot) expect(tabKeySlot(c.count, c.from, c.key, c.rtl), c.name).toBe(c.slot)
  })
  test(`reorder`, () => {
    for (const c of fixture.reorder) expect(tabReorder(c.visible, c.fromSlot, c.toSlot), c.name).toEqual(c.expect)
  })
  test(`moveTab is the preview order`, () => {
    expect(moveTab([0, 1, 2, 3], 0, 2)).toEqual([1, 2, 0, 3])
    expect(moveTab([0, 1, 2, 3], 3, 0)).toEqual([3, 0, 1, 2])
    expect(moveTab([0, 1], 0, 5)).toEqual([0, 1])
  })
})
