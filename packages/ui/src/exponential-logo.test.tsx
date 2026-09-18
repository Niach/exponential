import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { ExponentialLogo } from "./exponential-logo"

const svgOf = (container: HTMLElement) => container.querySelector(`svg`)!

describe(`ExponentialLogo`, () => {
  it(`draws at the asked size and takes a className`, () => {
    const { container } = render(<ExponentialLogo size={32} className="mr-2" />)
    const svg = svgOf(container)
    expect(svg.getAttribute(`width`)).toBe(`32`)
    expect(svg.getAttribute(`height`)).toBe(`32`)
    expect(svg.getAttribute(`viewBox`)).toBe(`0 0 100 100`)
    expect(svg.getAttribute(`class`)).toBe(`mr-2`)
  })

  it(`fills the mark dark by default and with the text color on light`, () => {
    const { container, rerender } = render(<ExponentialLogo />)
    expect(svgOf(container).querySelector(`circle[mask]`)!.getAttribute(`fill`))
      .toBe(`#222326`)
    rerender(<ExponentialLogo variant="light" />)
    expect(svgOf(container).querySelector(`circle[mask]`)!.getAttribute(`fill`))
      .toBe(`currentColor`)
  })

  it(`keeps its clip and mask ids unique across two instances`, () => {
    const { container } = render(
      <>
        <ExponentialLogo />
        <ExponentialLogo />
      </>
    )
    const ids = [...container.querySelectorAll(`clipPath, mask`)].map((node) =>
      node.getAttribute(`id`)
    )
    expect(ids).toHaveLength(4)
    expect(new Set(ids).size).toBe(4)
    // and each mark points at its OWN mask.
    const marks = [...container.querySelectorAll(`circle[mask]`)]
    expect(marks[0]!.getAttribute(`mask`)).not.toBe(
      marks[1]!.getAttribute(`mask`)
    )
  })
})
