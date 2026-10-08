import { fireEvent, render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { Toggle } from "./toggle"

describe(`Toggle`, () => {
  it(`renders a pressable button with data-slot`, () => {
    const { getByRole } = render(<Toggle aria-label="Bold">B</Toggle>)
    const toggle = getByRole(`button`)
    expect(toggle.dataset.slot).toBe(`toggle`)
    expect(toggle.getAttribute(`aria-pressed`)).toBe(`false`)
    fireEvent.click(toggle)
    expect(toggle.getAttribute(`aria-pressed`)).toBe(`true`)
  })
})
