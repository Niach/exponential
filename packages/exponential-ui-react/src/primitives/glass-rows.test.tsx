import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { GlassSectionHeader, ListRow } from "./glass-rows"

// EXP-862: the group band gained a FOLDABLE variant (the Agent page's "Recent").
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
        label="Recent"
        count={7}
        expanded={false}
        onToggle={onToggle}
      />
    )
    const band = screen.getByRole(`button`, { expanded: false })
    expect(band.textContent).toContain(`Recent`)
    expect(band.textContent).toContain(`7`)
    // Collapsed points right, expanded points down.
    const collapsedGlyph = band.querySelector(`svg`)?.innerHTML
    fireEvent.click(band)
    expect(onToggle).toHaveBeenCalledTimes(1)

    rerender(
      <GlassSectionHeader
        label="Recent"
        count={7}
        expanded
        onToggle={onToggle}
      />
    )
    const open = screen.getByRole(`button`, { expanded: true })
    expect(open.querySelector(`svg`)?.innerHTML).not.toBe(collapsedGlyph)
  })

  it(`shows a zero count rather than hiding the trailing slot`, () => {
    render(<GlassSectionHeader label="Recent" count={0} onToggle={vi.fn()} />)
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

describe(`ListRow density`, () => {
  it(`is the 12px-padded flat row by default`, () => {
    const { container } = render(<ListRow>Row</ListRow>)
    const row = container.querySelector(`[data-slot="list-row"]`)!
    expect(row.getAttribute(`data-density`)).toBe(`list`)
    expect(row.className).toContain(`p-3`)
    expect(row.className).not.toContain(`h-7`)
  })

  it(`compact is the sidebar's 28px one-line row`, () => {
    const { container } = render(
      <ListRow density="compact" interactive onClick={vi.fn()}>
        Row
      </ListRow>
    )
    const row = container.querySelector(`[data-slot="list-row"]`)!
    expect(row.getAttribute(`data-density`)).toBe(`compact`)
    expect(row.className).toContain(`h-7`)
    expect(row.className).toContain(`px-2`)
    expect(row.className).toContain(`py-0`)
    expect(row.className).toContain(`gap-2`)
    expect(row.className).toContain(`text-sm`)
  })

  it(`carries the density onto the child arm`, () => {
    const { container } = render(
      <ListRow density="compact" asChild>
        <a href="/x">Row</a>
      </ListRow>
    )
    const row = container.querySelector(`a[data-slot="list-row"]`)!
    expect(row.getAttribute(`data-density`)).toBe(`compact`)
    expect(row.className).toContain(`h-7`)
  })
})
