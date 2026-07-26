import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { PICKABLE_ICONS } from "@exp/icons"
import { IconSwatchGrid } from "@/components/ui/icon-swatch-grid"
import { BOARD_ICON_COMPONENTS, getBoardIcon } from "@/lib/board-icons"

// EXP-273 grew the curated set from 16 to 60 and moved the glyph lookup into
// the generated registry. These render for real (jsdom) rather than asserting
// on the data, because the failure mode that matters is "the name resolved to
// nothing and the swatch painted empty".

describe(`IconSwatchGrid`, () => {
  it(`renders one swatch per pickable icon, each with a real glyph`, () => {
    render(<IconSwatchGrid value="code" onChange={vi.fn()} />)
    const swatches = screen.getAllByRole(`button`)
    expect(swatches).toHaveLength(PICKABLE_ICONS.length)
    expect(swatches).toHaveLength(60)
    // Every swatch must actually paint an SVG — a missing component would
    // render an empty button and still pass a length check.
    for (const swatch of swatches) {
      expect(swatch.querySelector(`svg`)).not.toBeNull()
    }
  })

  it(`marks the selected icon and reports picks by name`, () => {
    const onChange = vi.fn()
    render(<IconSwatchGrid value="rocket" onChange={onChange} />)
    expect(screen.getByLabelText(`rocket`).getAttribute(`aria-pressed`)).toBe(
      `true`
    )
    fireEvent.click(screen.getByLabelText(`database`))
    expect(onChange).toHaveBeenCalledWith(`database`)
  })

  it(`filters by name and reports an empty result`, () => {
    render(<IconSwatchGrid value="code" onChange={vi.fn()} />)
    const search = screen.getByLabelText(`Search icons`)

    fireEvent.change(search, { target: { value: `git` } })
    // `git-branch` is the only pickable name containing "git".
    expect(screen.getAllByRole(`button`)).toHaveLength(1)
    expect(screen.getByLabelText(`git-branch`)).toBeTruthy()

    fireEvent.change(search, { target: { value: `zzzz` } })
    expect(screen.queryAllByRole(`button`)).toHaveLength(0)
    expect(screen.getByText(/No icon matches/)).toBeTruthy()
  })
})

describe(`board icon resolution`, () => {
  it(`maps every curated name to a distinct component`, () => {
    // The desktop's old hand-map collided `terminal` with `code` and
    // `lightbulb` with `star`; the registry must never reintroduce that.
    const components = PICKABLE_ICONS.map((n) => BOARD_ICON_COMPONENTS[n])
    expect(new Set(components).size).toBe(PICKABLE_ICONS.length)
  })

  it(`falls back by repo presence for an unset or unknown icon`, () => {
    expect(getBoardIcon({ icon: null, repositoryId: `r1` })).toBe(
      BOARD_ICON_COMPONENTS.code
    )
    expect(getBoardIcon({ icon: null, repositoryId: null })).toBe(
      BOARD_ICON_COMPONENTS[`square-kanban`]
    )
    // A name from a newer client must degrade, not throw.
    expect(getBoardIcon({ icon: `not-an-icon`, repositoryId: null })).toBe(
      BOARD_ICON_COMPONENTS[`square-kanban`]
    )
  })
})
