import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { GlassSectionHeader } from "@/components/ui/glass-rows"

// EXP-862: the group band gained a FOLDABLE variant (the Agent page's "Past").
// The plain band every other list uses must be untouched by it.
describe(`GlassSectionHeader`, () => {
  it(`stays a plain strip without a toggle`, () => {
    const { container } = render(<GlassSectionHeader label="Running" />)
    const band = container.querySelector(`[data-slot=glass-section-header]`)!
    expect(band.tagName).toBe(`DIV`)
    expect(band.getAttribute(`aria-expanded`)).toBeNull()
    expect(band.querySelector(`svg`)).toBeNull()
    expect(band.textContent).toBe(`Running`)
  })

  it(`folds: a chevron, the count, aria-expanded, and a click that toggles`, () => {
    const onToggle = vi.fn()
    const { rerender } = render(
      <GlassSectionHeader
        label="Past"
        count={7}
        expanded={false}
        onToggle={onToggle}
      />
    )
    const band = screen.getByRole(`button`, { expanded: false })
    expect(band.textContent).toContain(`Past`)
    expect(band.textContent).toContain(`7`)
    // Collapsed points right, expanded points down.
    const collapsedGlyph = band.querySelector(`svg`)?.innerHTML
    fireEvent.click(band)
    expect(onToggle).toHaveBeenCalledTimes(1)

    rerender(
      <GlassSectionHeader
        label="Past"
        count={7}
        expanded
        onToggle={onToggle}
      />
    )
    const open = screen.getByRole(`button`, { expanded: true })
    expect(open.querySelector(`svg`)?.innerHTML).not.toBe(collapsedGlyph)
  })

  it(`shows a zero count rather than hiding the trailing slot`, () => {
    render(<GlassSectionHeader label="Past" count={0} onToggle={vi.fn()} />)
    expect(screen.getByRole(`button`).textContent).toContain(`0`)
  })

  it(`keeps the leading glyph and the trailing slot`, () => {
    render(
      <GlassSectionHeader
        label="Reviews"
        leading={<span data-testid="leading" />}
        trailing={<span data-testid="trailing" />}
      />
    )
    expect(screen.getByTestId(`leading`)).not.toBeNull()
    expect(screen.getByTestId(`trailing`)).not.toBeNull()
  })
})
