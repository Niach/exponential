import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { RadioGroup, RadioGroupItem } from "./radio-group"

describe(`RadioGroup`, () => {
  it(`renders radios with data-slot and checks the default`, () => {
    const { container, getAllByRole } = render(
      <RadioGroup defaultValue="b">
        <RadioGroupItem value="a" aria-label="A" />
        <RadioGroupItem value="b" aria-label="B" />
      </RadioGroup>
    )
    expect(container.querySelector(`[data-slot="radio-group"]`)).not.toBeNull()
    const radios = getAllByRole(`radio`)
    expect(radios).toHaveLength(2)
    expect(radios[0].dataset.slot).toBe(`radio-group-item`)
    expect(radios[1].getAttribute(`aria-checked`)).toBe(`true`)
    expect(container.querySelector(`[data-slot="radio-group-indicator"]`)).not.toBeNull()
  })
})
