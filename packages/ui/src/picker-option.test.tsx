import { describe, expect, it } from "vitest"
import { CircleIcon } from "lucide-react"

import type { GlassPickerOption } from "./glass-rows"
import type { IssueOption } from "./issue-option"
import type { PickerOption } from "./picker-option"

// EXP-941: the four option shapes collapsed into ONE. These are compile-time
// assertions with a runtime tail — if `IssueOption` ever stops being a
// `PickerOption`, or `GlassPickerOption` stops being a slice of it, this file
// fails to typecheck, which is the whole point of the exercise.

describe(`PickerOption`, () => {
  it(`is what an IssueOption is, with three slots made required`, () => {
    const status: IssueOption<`backlog`> = {
      value: `backlog`,
      label: `Backlog`,
      icon: CircleIcon,
      color: `text-muted-foreground`,
    }
    // The narrowing direction: every IssueOption IS a PickerOption.
    const asPicker: PickerOption<`backlog`> = status
    expect(asPicker.value).toBe(`backlog`)
    expect(asPicker.label).toBe(`Backlog`)

    // …and the shared optional slots are available on it.
    const rich: IssueOption<`urgent`> = {
      value: `urgent`,
      label: `Urgent`,
      icon: CircleIcon,
      color: `text-destructive`,
      colorHex: `#ef4444`,
      keywords: [`p0`, `now`],
      hint: `default`,
      dot: `#ef4444`,
      disabled: true,
    }
    expect(rich.keywords).toEqual([`p0`, `now`])
  })

  it(`is what a GlassPickerOption is a slice of`, () => {
    const row: GlassPickerOption = {
      value: `main`,
      label: `main`,
      disabled: false,
    }
    const asPicker: PickerOption = row
    expect(asPicker.value).toBe(`main`)

    // A full PickerOption still fits a row picker — the slice is a subset.
    const full: PickerOption = {
      value: `dev`,
      label: `dev`,
      icon: CircleIcon,
      hint: `default`,
    }
    const narrowed: GlassPickerOption = full
    expect(narrowed.label).toBe(`dev`)
  })

  it(`carries a ReactNode label, not just a string`, () => {
    const option: PickerOption = {
      value: `x`,
      label: <span>rich</span>,
    }
    expect(option.label).toBeTruthy()
  })
})
