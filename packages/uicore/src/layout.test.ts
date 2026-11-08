import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/shell-layout.json" with { type: "json" }
import { LAYOUT, sidebarWidth, widgetSize } from "./layout"

describe(`shell-layout.json`, () => {
  test(`layout.ts reads every number of the fixture`, () => {
    expect(LAYOUT).toEqual(fixture as unknown as typeof LAYOUT)
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

  test(`a widget takes its cells of the grid`, () => {
    expect(widgetSize(`s`)).toEqual({ width: 160, height: 160 })
    expect(widgetSize(`m`)).toEqual({ width: 336, height: 160 })
    expect(widgetSize(`l`)).toEqual({ width: 336, height: 336 })
    expect(widgetSize(`xl`)).toEqual({ width: 160, height: 160 })
  })

  test(`the rail is the collapsed sidebar`, () => {
    expect(sidebarWidth(false)).toBe(272)
    expect(sidebarWidth(true)).toBe(56)
  })
})
