import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { BoardGlyph } from "./board-glyph"

// EXP-449: the one board glyph — the board's own icon tinted with its color,
// never an anonymous dot.

describe(`BoardGlyph`, () => {
  it(`tints the glyph with the board's color`, () => {
    const { container } = render(
      <BoardGlyph board={{ icon: `rocket`, color: `#ff8800` }} />
    )
    const svg = container.querySelector(`svg`)!
    expect(svg.style.color).toBe(`rgb(255, 136, 0)`)
  })

  it(`leaves an uncolored board on the inherited color`, () => {
    const { container } = render(<BoardGlyph board={{ icon: `rocket` }} />)
    expect(container.querySelector(`svg`)!.style.color).toBe(``)
  })

  it(`merges the caller's className over its own box`, () => {
    const { container } = render(
      <BoardGlyph board={{ icon: `rocket` }} className="size-5 text-primary" />
    )
    const className = container.querySelector(`svg`)!.getAttribute(`class`)!
    expect(className).toContain(`shrink-0`)
    expect(className).toContain(`size-5`)
    expect(className).not.toContain(`size-4`)
    expect(className).toContain(`text-primary`)
  })

  it(`falls back on repo presence when the board has no icon`, () => {
    const { container: withRepo } = render(
      <BoardGlyph board={{ repositoryId: `repo-1` }} />
    )
    const { container: without } = render(<BoardGlyph board={{}} />)
    expect(withRepo.querySelector(`svg`)!.innerHTML).not.toBe(
      without.querySelector(`svg`)!.innerHTML
    )
  })
})
