import { describe, expect, it } from "vitest"
import { CircleIcon } from "lucide-react"

import type { PickerOption } from "./picker-option"

// EXP-941: the four option shapes collapsed into ONE; EXP-958 removed the two
// narrowings (`IssueOption`, `GlassPickerOption`) with the shells that needed
// them. These are compile-time assertions with a runtime tail — the app's
// status tables narrow `PickerOption` on their own side now, and a row built
// with every slot must still BE a PickerOption or this file stops compiling.

describe(`PickerOption`, () => {
  it(`carries every slot a picker row may draw`, () => {
    const rich: PickerOption<`urgent`> = {
      value: `urgent`,
      label: `Urgent`,
      icon: CircleIcon,
      color: `text-destructive`,
      colorHex: `#ef4444`,
      keywords: [`p0`, `now`],
      hint: `default`,
      dot: `#ef4444`,
      disabled: true,
      checked: `indeterminate`,
    }
    expect(rich.keywords).toEqual([`p0`, `now`])
    expect(rich.checked).toBe(`indeterminate`)
  })

  it(`needs only a value and a label`, () => {
    const bare: PickerOption = { value: `main`, label: `main` }
    expect(bare.icon).toBeUndefined()
  })

  it(`carries a ReactNode label, not just a string`, () => {
    const option: PickerOption = {
      value: `x`,
      label: <span>rich</span>,
    }
    expect(option.label).toBeTruthy()
  })
})
