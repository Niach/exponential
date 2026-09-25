import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { Picker, pickerItemKeywords, type PickerItem } from "./picker"

// EXP-1029 contract, EXP-1021 implementation — the picker primitive. The
// presentation table below IS the contract; the IDE, iOS and Android
// primitives carry the same case names.

// Radix positions its content with ResizeObserver and cmdk scrolls the active
// row into view; jsdom has neither.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const items: PickerItem[] = [
  { value: `a`, label: `Alpha` },
  { value: `b`, label: `Beta`, description: `second` },
  { value: `c`, label: `Gamma`, disabled: true },
]

const trigger = <button type="button">Pick one</button>

const open = () => {
  act(() => {
    fireEvent.click(screen.getByRole(`button`, { name: `Pick one` }))
  })
}

const rows = () =>
  Array.from(document.querySelectorAll(`[data-slot=command-item]`))

/** The viewport `useIsMobile` reads, for the phone arm of a case. */
function setViewport(width: number) {
  Object.defineProperty(window, `innerWidth`, {
    configurable: true,
    writable: true,
    value: width,
  })
}

describe(`Picker (EXP-1029 contract)`, () => {
  it(`renders its trigger under the picker marker`, () => {
    const { container } = render(
      <Picker
        mode="single"
        items={items}
        value="a"
        onChange={vi.fn()}
        trigger={trigger}
      />
    )
    const marker = container.querySelector(`[data-slot="picker"]`)
    expect(marker).not.toBeNull()
    expect(marker?.getAttribute(`data-picker-mode`)).toBe(`single`)
    expect(screen.getByRole(`button`, { name: `Pick one` })).toBeTruthy()
  })

  it(`matches a row on its keywords, else on a string label`, () => {
    expect(pickerItemKeywords({ value: `a`, label: `Alpha` })).toEqual([`Alpha`])
    expect(
      pickerItemKeywords({ value: `a`, label: <b>Alpha</b>, keywords: [`APP-1`] })
    ).toEqual([`APP-1`])
    expect(pickerItemKeywords({ value: `a`, label: <b>Alpha</b> })).toEqual([])
  })
})

describe(`Picker presentation (EXP-1029 → EXP-1021)`, () => {
  it(`opens a popover anchored at the trigger on a pointer device`, () => {
    setViewport(1280)
    render(
      <Picker mode="single" items={items} value={null} onChange={vi.fn()} trigger={trigger} />
    )
    open()
    expect(document.querySelector(`[data-slot=popover-content]`)).not.toBeNull()
    expect(document.querySelector(`[data-slot=sheet-content]`)).toBeNull()
    expect(rows()).toHaveLength(3)
  })

  it(`opens a bottom sheet of plain rows on a phone — no cards inside the sheet`, () => {
    setViewport(390)
    render(
      <Picker
        mode="single"
        items={items}
        value={null}
        onChange={vi.fn()}
        mobileTitle="Pick"
        trigger={trigger}
      />
    )
    open()
    const sheet = document.querySelector(`[data-slot=sheet-content]`)
    expect(sheet).not.toBeNull()
    expect(sheet?.getAttribute(`data-side`)).toBe(`bottom`)
    expect(screen.getByText(`Pick`)).toBeTruthy()
    // The rows sit on the sheet itself: no glass card, no section, no
    // bordered row shell between the sheet and the list.
    expect(sheet?.querySelector(`[data-slot=card]`)).toBeNull()
    expect(sheet?.querySelector(`[data-slot=glass-card]`)).toBeNull()
    expect(rows()).toHaveLength(3)
    setViewport(1280)
  })

  it(`single mode closes on a pick and reports the value`, () => {
    setViewport(1280)
    const onChange = vi.fn()
    render(
      <Picker mode="single" items={items} value={null} onChange={onChange} trigger={trigger} />
    )
    open()
    act(() => {
      fireEvent.click(screen.getByText(`Beta`))
    })
    expect(onChange).toHaveBeenCalledWith(`b`)
    expect(rows()).toHaveLength(0)
  })

  it(`multi mode toggles without closing and reports the whole set`, () => {
    setViewport(1280)
    const onChange = vi.fn()
    render(
      <Picker mode="multi" items={items} value={[`a`]} onChange={onChange} trigger={trigger} />
    )
    open()
    act(() => {
      fireEvent.click(screen.getByText(`Beta`))
    })
    expect(onChange).toHaveBeenCalledWith([`a`, `b`])
    expect(rows()).toHaveLength(3)
    // A picked row toggles back off, and the whole new set is reported.
    act(() => {
      fireEvent.click(screen.getByText(`Alpha`))
    })
    expect(onChange).toHaveBeenLastCalledWith([])
  })

  it(`multi mode marks picked rows by the highlight colour, never a circle`, () => {
    setViewport(1280)
    render(
      <Picker mode="multi" items={items} value={[`a`]} onChange={vi.fn()} trigger={trigger} />
    )
    open()
    // No circle pair, no check: the row's own highlight IS the mark.
    expect(document.querySelectorAll(`[data-selected-glyph]`)).toHaveLength(0)
    const picked = rows().filter(
      (row) => row.getAttribute(`data-picked`) === `true`
    )
    expect(picked).toHaveLength(1)
    expect(picked[0]!.textContent).toContain(`Alpha`)
    expect(picked[0]!.className).toContain(`bg-glass-active`)
    expect(picked[0]!.className).toContain(`ring-glass-stroke-active`)
  })

  it(`single mode marks the picked row with a trailing check, never a wash`, () => {
    setViewport(1280)
    // EXP-1021 review r3 — the ×4 twin of the multi case above. The two
    // rules are one language on all four clients:
    //   single  a trailing `ui-check` on the picked row (EXP-957's rule,
    //           matched to the natives), and NO wash — the wash means
    //           "picked" only where there is more than one pick to see.
    //   multi   the row's own highlight, never a glyph.
    // iOS and Android had drifted to the wash for BOTH, which made a single
    // pick on a phone read like a multi pick on a pointer device.
    render(
      <Picker mode="single" items={items} value="a" onChange={vi.fn()} trigger={trigger} />
    )
    open()
    const checks = document.querySelectorAll(`[data-selected-glyph="check"]`)
    expect(checks).toHaveLength(1)
    const picked = rows().find((row) => row.textContent?.includes(`Alpha`))!
    expect(picked.querySelector(`[data-selected-glyph="check"]`)).not.toBeNull()
    // The single arm never borrows the multi arm's mark.
    expect(picked.getAttribute(`data-picked`)).toBe(`true`)
    expect(picked.className).not.toContain(`ring-glass-stroke-active`)
    // …and never the circle pair the multi arm retired.
    expect(document.querySelectorAll(`[data-selected-glyph="selected"]`)).toHaveLength(0)
    expect(document.querySelectorAll(`[data-selected-glyph="unselected"]`)).toHaveLength(0)
  })

  it(`a phone sheet closes on swipe down`, () => {
    setViewport(390)
    // The gesture only arms itself under the CSS `sm` breakpoint, and it
    // waits out the slide-out before it closes (EXP-687).
    const matchMedia = window.matchMedia
    window.matchMedia = ((query: string) =>
      ({
        matches: query.includes(`639px`),
        media: query,
        addEventListener: () => {},
        removeEventListener: () => {},
      }) as unknown as MediaQueryList) as typeof window.matchMedia
    vi.useFakeTimers()
    const onOpenChange = vi.fn()
    try {
      render(
        <Picker
          mode="single"
          items={items}
          value={null}
          onChange={vi.fn()}
          mobileTitle="Pick"
          onOpenChange={onOpenChange}
          trigger={trigger}
        />
      )
      open()
      const grabber = document.querySelector(`[data-slot=sheet-grabber]`)
      expect(grabber).not.toBeNull()
      act(() => {
        fireEvent.pointerDown(grabber!, { clientY: 0, pointerId: 1 })
        fireEvent.pointerMove(grabber!, { clientY: 200, pointerId: 1 })
        fireEvent.pointerUp(grabber!, { clientY: 200, pointerId: 1 })
      })
      act(() => {
        vi.advanceTimersByTime(400)
      })
      expect(onOpenChange).toHaveBeenLastCalledWith(false)
    } finally {
      vi.useRealTimers()
      window.matchMedia = matchMedia
      setViewport(1280)
    }
  })

  it(`search filters rows by label and keywords`, () => {
    setViewport(1280)
    render(
      <Picker
        mode="single"
        items={[
          { value: `a`, label: `Alpha`, keywords: [`APP-1`, `Alpha`] },
          { value: `b`, label: `Beta`, keywords: [`APP-2`, `Beta`] },
        ]}
        value={null}
        onChange={vi.fn()}
        search
        trigger={trigger}
      />
    )
    open()
    const field = document.querySelector(`[data-slot=command-input]`)!
    act(() => {
      fireEvent.change(field, { target: { value: `Bet` } })
    })
    expect(rows()).toHaveLength(1)
    act(() => {
      fireEvent.change(field, { target: { value: `APP-1` } })
    })
    expect(rows()).toHaveLength(1)
    expect(rows()[0]!.textContent).toContain(`Alpha`)
  })

  it(`a disabled row renders but never picks`, () => {
    setViewport(1280)
    const onChange = vi.fn()
    render(
      <Picker mode="single" items={items} value={null} onChange={onChange} trigger={trigger} />
    )
    open()
    const gamma = rows().find((row) => row.textContent?.includes(`Gamma`))!
    expect(gamma.getAttribute(`data-disabled`)).toBe(`true`)
    act(() => {
      fireEvent.click(gamma)
    })
    expect(onChange).not.toHaveBeenCalled()
  })

  it(`emptyText shows for no items and for an empty search`, () => {
    setViewport(1280)
    const { unmount } = render(
      <Picker
        mode="single"
        items={[]}
        value={null}
        onChange={vi.fn()}
        emptyText="No boards"
        trigger={trigger}
      />
    )
    open()
    expect(screen.getByText(`No boards`)).toBeTruthy()
    unmount()

    render(
      <Picker
        mode="single"
        items={items}
        value={null}
        onChange={vi.fn()}
        search
        emptyText="No boards"
        trigger={trigger}
      />
    )
    open()
    act(() => {
      fireEvent.change(document.querySelector(`[data-slot=command-input]`)!, {
        target: { value: `zzz` },
      })
    })
    expect(screen.getByText(`No boards`)).toBeTruthy()
  })

  it(`className styles the SURFACE, not the caller's own trigger`, () => {
    setViewport(1280)
    // Regression (EXP-1021 review): `className` used to be forwarded to
    // `Combobox`, which applies it to its OWN default trigger — and `Picker`
    // always supplies `renderTrigger`, so it was dropped on the floor. The
    // icon picker's `w-auto` (its grid is wider than any of the four widths)
    // silently did nothing.
    render(
      <Picker
        mode="single"
        items={items}
        value={null}
        onChange={vi.fn()}
        className="w-auto"
        trigger={trigger}
      />
    )
    open()
    const surface = document.querySelector(`[data-slot=popover-content]`)
    expect(surface).not.toBeNull()
    expect(surface!.className).toContain(`w-auto`)
    // …and it wins over the width, or it could never hug a fixed-size body.
    expect(surface!.className.indexOf(`w-auto`)).toBeGreaterThan(
      surface!.className.indexOf(`w-[16rem]`)
    )
  })

  it(`a row draws its icon in its colour and its description muted`, () => {
    setViewport(1280)
    const Glyph = (props: { className?: string }) => (
      <svg data-testid="glyph" {...props} />
    )
    render(
      <Picker
        mode="single"
        items={[
          { value: `a`, label: `Alpha`, icon: Glyph as never, color: `#3B82F6` },
          { value: `b`, label: `Beta`, color: `#EF4444` },
          { value: `c`, label: `Gamma`, description: `ada@example.com` },
        ]}
        value={null}
        onChange={vi.fn()}
        trigger={trigger}
      />
    )
    open()
    // A colour WITH a glyph tints the glyph …
    const glyph = screen.getByTestId(`glyph`)
    expect(glyph.getAttribute(`style`)).toContain(`rgb(59, 130, 246)`)
    // … a colour WITHOUT one draws the row's dot (a label).
    const dot = document.querySelector(`[data-slot=picker-dot]`)
    expect(dot).not.toBeNull()
    expect(dot!.getAttribute(`style`)).toContain(`rgb(239, 68, 68)`)
    // The description is the muted second line.
    const description = document.querySelector(`[data-slot=picker-description]`)
    expect(description?.textContent).toBe(`ada@example.com`)
    expect(description?.className).toContain(`text-muted-foreground`)
  })
})
