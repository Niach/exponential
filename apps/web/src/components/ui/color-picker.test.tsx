import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { ColorPicker } from "@/components/ui/color-picker"
import { LABEL_COLORS } from "@/lib/label-colors"

// EXP-862: the colour picker is the icon picker's twin — a single swatch
// until clicked, the palette lives in a popover, never inline in the form.

describe(`ColorPicker`, () => {
  it(`renders only the trigger until opened, then the full palette`, () => {
    render(<ColorPicker value={LABEL_COLORS[0]!} onChange={vi.fn()} />)
    expect(screen.getAllByRole(`button`)).toHaveLength(1)
    const trigger = screen.getByLabelText(`Color: ${LABEL_COLORS[0]}`)
    // The trigger previews the pick by painting it.
    expect(trigger.querySelector(`span`)?.style.backgroundColor).toBeTruthy()
    fireEvent.click(trigger)
    expect(screen.getAllByRole(`button`)).toHaveLength(LABEL_COLORS.length + 1)
  })

  it(`reports the pick and closes`, () => {
    const onChange = vi.fn()
    render(<ColorPicker value={LABEL_COLORS[0]!} onChange={onChange} />)
    fireEvent.click(screen.getByLabelText(`Color: ${LABEL_COLORS[0]}`))
    fireEvent.click(screen.getByRole(`button`, { name: LABEL_COLORS[3]! }))
    expect(onChange).toHaveBeenCalledWith(LABEL_COLORS[3])
    expect(screen.getAllByRole(`button`)).toHaveLength(1)
  })

  it(`shares the icon picker's trigger shape, dashed while unset`, () => {
    const { rerender } = render(<ColorPicker value="" onChange={vi.fn()} />)
    const empty = screen.getByLabelText(`Pick a color`)
    expect(empty.className).toContain(`h-9 w-9`)
    expect(empty.className).toContain(`rounded-md`)
    expect(empty.className).toContain(`border-dashed`)
    rerender(<ColorPicker value={LABEL_COLORS[0]!} onChange={vi.fn()} />)
    expect(
      screen.getByLabelText(`Color: ${LABEL_COLORS[0]}`).className
    ).not.toContain(`border-dashed`)
  })

  it(`restricts the grid to the colours it is given`, () => {
    const colors = [LABEL_COLORS[0]!, LABEL_COLORS[1]!]
    render(
      <ColorPicker value={colors[0]!} onChange={vi.fn()} colors={colors} />
    )
    fireEvent.click(screen.getByLabelText(`Color: ${colors[0]}`))
    expect(screen.getAllByRole(`button`)).toHaveLength(colors.length + 1)
  })
})
