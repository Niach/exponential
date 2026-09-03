import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { TeamAvatar } from "@/components/team/team-avatar"

// The team mark is a SQUARE at a quarter radius in the primary accent — the
// one thing that stops a team from reading like a member (EXP-698 r5).
describe(`TeamAvatar`, () => {
  it(`draws the first letter, uppercased`, () => {
    const { container } = render(<TeamAvatar name="acme rockets" />)
    expect(container.textContent).toBe(`A`)
  })

  it(`falls back to E for a nameless team`, () => {
    expect(render(<TeamAvatar name={undefined} />).container.textContent).toBe(
      `E`
    )
    expect(render(<TeamAvatar name="  " />).container.textContent).toBe(`E`)
  })

  it(`squares itself at the requested size with a quarter radius`, () => {
    const { container } = render(<TeamAvatar name="Acme" size={28} />)
    const mark = container.firstElementChild as HTMLElement
    expect(mark.style.width).toBe(`28px`)
    expect(mark.style.height).toBe(`28px`)
    expect(mark.style.borderRadius).toBe(`7px`)
  })

  it(`paints the accent, never a hashed hue`, () => {
    const { container } = render(<TeamAvatar name="Acme" />)
    const mark = container.firstElementChild as HTMLElement
    expect(mark.className).toContain(`bg-primary`)
    expect(mark.className).toContain(`text-primary-foreground`)
  })
})
