import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/shell-layout.json" with { type: "json" }
import { LAYOUT, sidebarWidth } from "./layout"

describe(`shell-layout.json`, () => {
  test(`layout.ts reads every number of the fixture`, () => {
    expect(LAYOUT).toEqual(fixture)
    expect(LAYOUT.sidebarWidth).toBe(272)
    expect(LAYOUT.railWidth).toBe(56)
    expect(LAYOUT.taskbarHeight).toBe(48)
    expect(LAYOUT.panelMargin).toBe(10)
    expect(LAYOUT.panelRadius).toBe(12)
    expect(LAYOUT.sidebarRowHeight).toBe(28)
    expect(LAYOUT.sectionLabelHeight).toBe(24)
    expect(LAYOUT.taskbarButtonSize).toBe(40)
    expect(LAYOUT.appletButtonSize).toBe(32)
    expect(LAYOUT.startMenu).toEqual({ width: 760, height: 620 })
    expect(LAYOUT.previewsTile).toEqual({ width: 184, height: 110 })
  })

  test(`the rail is the collapsed sidebar`, () => {
    expect(sidebarWidth(false)).toBe(272)
    expect(sidebarWidth(true)).toBe(56)
  })
})
