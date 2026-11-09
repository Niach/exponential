import { describe, expect, test } from "bun:test"
import { isPickerPick, isPickerPopover } from "./picker"

describe(`picker (#EXP-1760)`, () => {
  test(`the root Popover of either picker`, () => {
    expect(isPickerPopover({ macro: `IconPicker`, part: `root` })).toBe(true)
    expect(isPickerPopover({ macro: `ColorPicker`, part: `root` })).toBe(true)
    expect(isPickerPopover({ macro: `IconPicker`, part: `cell` })).toBe(false)
    expect(isPickerPopover({ macro: `Select`, part: `root` })).toBe(false)
    expect(isPickerPopover(undefined)).toBe(false)
  })
  test(`a cell, a swatch and the No icon row pick; the trigger does not`, () => {
    expect(isPickerPick({ macro: `IconPicker`, part: `cell` })).toBe(true)
    expect(isPickerPick({ macro: `IconPicker`, part: `none` })).toBe(true)
    expect(isPickerPick({ macro: `ColorPicker`, part: `swatch` })).toBe(true)
    expect(isPickerPick({ macro: `IconPicker`, part: `trigger` })).toBe(false)
    expect(isPickerPick({ macro: `ColorPicker`, part: `trigger` })).toBe(false)
    expect(isPickerPick(undefined)).toBe(false)
  })
})
