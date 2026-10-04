import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { BrandHeading } from "./brand-heading"

describe(`BrandHeading`, () => {
  it(`draws the mark over the title and nothing else`, () => {
    const { container } = render(<BrandHeading title="Continue to Exponential" />)
    const svg = container.querySelector(`svg`)
    expect(svg?.getAttribute(`width`)).toBe(`56`)
    expect(screen.getByRole(`heading`, { level: 1 }).textContent).toBe(
      `Continue to Exponential`
    )
    expect(container.querySelector(`p`)).toBeNull()
  })

  it(`adds a description line only when one is given`, () => {
    render(<BrandHeading title="Check your email" description="We sent a code." />)
    expect(screen.getByText(`We sent a code.`)).toBeTruthy()
  })
})
