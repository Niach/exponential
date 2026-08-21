import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { PICKABLE_ICONS } from "@exp/icons"
import { IconPicker } from "@/components/ui/icon-picker"

// EXP-575: the picker is a single swatch until clicked — the 60-glyph grid
// lives in a popover, never inline in the form.

describe(`IconPicker`, () => {
  it(`renders only the trigger until opened, then the full grid`, () => {
    render(<IconPicker value="rocket" onChange={vi.fn()} />)
    expect(screen.getAllByRole(`button`)).toHaveLength(1)
    const trigger = screen.getByLabelText(`Icon: rocket`)
    expect(trigger.querySelector(`svg`)).not.toBeNull()
    fireEvent.click(trigger)
    expect(screen.getAllByRole(`button`)).toHaveLength(PICKABLE_ICONS.length + 1)
  })

  it(`reports the pick and closes`, () => {
    const onChange = vi.fn()
    render(<IconPicker value="rocket" onChange={onChange} />)
    fireEvent.click(screen.getByLabelText(`Icon: rocket`))
    fireEvent.click(screen.getByLabelText(`database`))
    expect(onChange).toHaveBeenCalledWith(`database`)
  })

  it(`offers "No icon" only when allowed and something is set`, () => {
    const onChange = vi.fn()
    const { rerender } = render(
      <IconPicker value="rocket" onChange={onChange} allowsNone />
    )
    fireEvent.click(screen.getByLabelText(`Icon: rocket`))
    fireEvent.click(screen.getByText(`No icon`))
    expect(onChange).toHaveBeenCalledWith(``)

    rerender(<IconPicker value="" onChange={onChange} allowsNone />)
    fireEvent.click(screen.getByLabelText(`Pick an icon`))
    expect(screen.queryByText(`No icon`)).toBeNull()
  })
})
