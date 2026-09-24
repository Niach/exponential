import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { Picker, pickerItemKeywords, type PickerItem } from "./picker"

// EXP-1029 contract — the picker primitive. The live cases pin what the stub
// already does; the skipped table is the presentation contract EXP-1021
// implements and un-skips (the IDE, iOS and Android primitives carry the same
// case names).

const items: PickerItem[] = [
  { value: `a`, label: `Alpha` },
  { value: `b`, label: `Beta`, description: `second` },
  { value: `c`, label: `Gamma`, disabled: true },
]

describe(`Picker (EXP-1029 contract)`, () => {
  it(`renders its trigger under the picker marker`, () => {
    const { container } = render(
      <Picker
        mode="single"
        items={items}
        value="a"
        onChange={vi.fn()}
        trigger={<button type="button">Pick one</button>}
      />
    )
    const marker = container.querySelector(`[data-slot="picker"]`)
    expect(marker).not.toBeNull()
    expect(marker?.getAttribute(`data-picker-mode`)).toBe(`single`)
    expect(screen.getByRole(`button`, { name: `Pick one` })).toBeTruthy()
  })

  it(`matches a row on its keywords, else on a string label`, () => {
    expect(pickerItemKeywords({ value: `a`, label: `Alpha` })).toEqual([`Alpha`])
    expect(
      pickerItemKeywords({ value: `a`, label: <b>Alpha</b>, keywords: [`APP-1`] })
    ).toEqual([`APP-1`])
    expect(pickerItemKeywords({ value: `a`, label: <b>Alpha</b> })).toEqual([])
  })
})

describe.skip(`Picker presentation (EXP-1029 → EXP-1021)`, () => {
  it(`opens a popover anchored at the trigger on a pointer device`, () => {})
  it(`opens a bottom sheet of plain rows on a phone — no cards inside the sheet`, () => {})
  it(`single mode closes on a pick and reports the value`, () => {})
  it(`multi mode toggles without closing and reports the whole set`, () => {})
  it(`multi mode marks picked rows by the highlight colour, never a circle`, () => {})
  it(`a phone sheet closes on swipe down`, () => {})
  it(`search filters rows by label and keywords`, () => {})
  it(`a disabled row renders but never picks`, () => {})
  it(`emptyText shows for no items and for an empty search`, () => {})
  it(`a row draws its icon in its colour and its description muted`, () => {})
})
