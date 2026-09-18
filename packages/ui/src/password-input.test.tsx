import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { PasswordInput } from "./password-input"

describe(`PasswordInput`, () => {
  it(`hides the value until the toggle reveals it`, () => {
    render(<PasswordInput aria-label="Password" defaultValue="hunter2" />)
    const field = screen.getByLabelText(`Password`) as HTMLInputElement
    expect(field.type).toBe(`password`)

    fireEvent.click(screen.getByRole(`button`, { name: `Show password` }))
    expect(field.type).toBe(`text`)

    fireEvent.click(screen.getByRole(`button`, { name: `Hide password` }))
    expect(field.type).toBe(`password`)
    expect(screen.getByRole(`button`, { name: `Show password` })).toBeTruthy()
  })

  it(`keeps the toggle out of the tab order`, () => {
    render(<PasswordInput aria-label="Password" />)
    expect(
      screen.getByRole(`button`, { name: `Show password` }).tabIndex
    ).toBe(-1)
  })
})
