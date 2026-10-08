import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { Spinner } from "./spinner"

describe(`Spinner`, () => {
  it(`renders a status svg with data-slot`, () => {
    const { getByRole } = render(<Spinner className="size-6" />)
    const spinner = getByRole(`status`)
    expect(spinner.getAttribute(`data-slot`)).toBe(`spinner`)
    expect(spinner.getAttribute(`aria-label`)).toBe(`Loading`)
    expect(spinner.getAttribute(`class`)).toContain(`size-6`)
  })
})
