import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { Slider } from "./slider"

describe(`Slider`, () => {
  it(`renders track, range and one thumb per value with data-slot`, () => {
    const { container } = render(<Slider defaultValue={[20, 60]} max={100} />)
    expect(container.querySelector(`[data-slot="slider"]`)).not.toBeNull()
    expect(container.querySelector(`[data-slot="slider-track"]`)).not.toBeNull()
    expect(container.querySelector(`[data-slot="slider-range"]`)).not.toBeNull()
    expect(container.querySelectorAll(`[data-slot="slider-thumb"]`)).toHaveLength(2)
  })
})
