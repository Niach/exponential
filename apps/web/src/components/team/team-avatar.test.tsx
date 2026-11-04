import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { TeamAvatar } from "@exp/ui"

// #EXP-1645 (team-image.json): the team's image, else the group glyph; never
// a letter, never a chip.
describe(`TeamAvatar`, () => {
  it(`draws the group glyph without an image, and no letter`, () => {
    for (const iconUrl of [null, undefined, ``, `  `]) {
      const { container, unmount } = render(<TeamAvatar iconUrl={iconUrl} />)
      expect(container.textContent).toBe(``)
      expect(container.querySelector(`svg`)).not.toBeNull()
      expect(container.querySelector(`img`)).toBeNull()
      unmount()
    }
  })

  it(`masks the image at a quarter of its side, with no border or fill`, () => {
    const { container } = render(<TeamAvatar iconUrl="/api/attachments/a" size={28} />)
    const mark = container.firstElementChild as HTMLElement
    expect(mark.style.width).toBe(`28px`)
    expect(mark.className).not.toMatch(/bg-|border/)
    const image = container.querySelector(`img`) as HTMLImageElement
    expect(image.getAttribute(`src`)).toBe(`/api/attachments/a`)
    expect(image.style.borderRadius).toBe(`7px`)
    expect(image.className).not.toMatch(/bg-|border/)
  })
})
