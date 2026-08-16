import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"
import { LABEL_COLORS } from "@/lib/label-colors"

describe(`ColorSwatchGrid`, () => {
  it(`renders one swatch per color, each painting its own hex`, () => {
    render(<ColorSwatchGrid value={LABEL_COLORS[0]} onChange={vi.fn()} />)
    const swatches = screen.getAllByRole(`button`)
    expect(swatches).toHaveLength(LABEL_COLORS.length)
    // jsdom normalizes hex to `rgb(...)`, so assert every swatch paints SOME
    // colour and that no two share one — a mis-keyed map would collapse them.
    const painted = swatches.map(
      (swatch) => swatch.querySelector(`span`)?.style.backgroundColor ?? ``
    )
    expect(painted.every(Boolean)).toBe(true)
    expect(new Set(painted).size).toBe(LABEL_COLORS.length)
  })

  it(`marks the selected swatch inside its own box, never with a ring offset`, () => {
    render(<ColorSwatchGrid value={LABEL_COLORS[0]} onChange={vi.fn()} />)
    const [selected, other] = screen.getAllByRole(`button`)

    expect(selected.getAttribute(`aria-pressed`)).toBe(`true`)
    expect(other.getAttribute(`aria-pressed`)).toBe(`false`)
    // EXP-524: a `ring-offset` halo paints past the border box and the
    // nearest scroll container (a DialogBody) clips it — which cut the FIRST
    // swatch in half. The selection must stay inside the swatch's own box.
    expect(selected.className).not.toContain(`ring-offset`)
    expect(selected.className).toContain(`border-foreground`)
    expect(other.className).toContain(`border-transparent`)
  })

  it(`reports the clicked color`, () => {
    const onChange = vi.fn()
    render(<ColorSwatchGrid value={LABEL_COLORS[0]} onChange={onChange} />)
    fireEvent.click(screen.getByRole(`button`, { name: LABEL_COLORS[3] }))
    expect(onChange).toHaveBeenCalledWith(LABEL_COLORS[3])
  })
})
