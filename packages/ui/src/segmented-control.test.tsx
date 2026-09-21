import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { conceptIcon } from "./icons.generated"
import { SegmentedControl } from "./segmented-control"
import { SEGMENTED_LIST, SEGMENTED_TAB } from "./tabs"

// EXP-941: the capsule was a class recipe eight surfaces wired by hand. It is
// a component now — and it still reads the SAME constants, so nothing that
// asserts on those strings (`mobile-detail-header.test.ts`) can drift.

const OPTIONS = [
  { value: `inbox` as const, label: `Inbox` },
  { value: `mine` as const, label: `My issues` },
]

describe(`SegmentedControl`, () => {
  it(`renders one tab per option and marks the active one`, () => {
    render(
      <SegmentedControl
        value="mine"
        onValueChange={vi.fn()}
        options={OPTIONS}
      />
    )
    const tabs = screen.getAllByRole(`tab`)
    expect(tabs).toHaveLength(2)
    expect(tabs.map((tab) => tab.textContent)).toEqual([`Inbox`, `My issues`])
    expect(tabs[1]!.getAttribute(`data-state`)).toBe(`active`)
    expect(tabs[0]!.getAttribute(`data-state`)).toBe(`inactive`)
  })

  it(`reports the segment that was clicked`, () => {
    const onValueChange = vi.fn()
    render(
      <SegmentedControl
        value="inbox"
        onValueChange={onValueChange}
        options={OPTIONS}
      />
    )
    // Radix activates a tab on mousedown, not on click.
    fireEvent.mouseDown(screen.getByRole(`tab`, { name: `My issues` }), {
      button: 0,
      ctrlKey: false,
    })
    expect(onValueChange).toHaveBeenCalledWith(`mine`)
  })

  it(`uses the shared trigger and list recipes, never a local size`, () => {
    render(
      <SegmentedControl value="inbox" onValueChange={vi.fn()} options={OPTIONS} />
    )
    expect(screen.getByRole(`tablist`).className).toContain(SEGMENTED_LIST)
    expect(screen.getAllByRole(`tab`)[0]!.className).toContain(SEGMENTED_TAB)
  })

  it(`draws a glyph when an option brings one`, () => {
    render(
      <SegmentedControl
        value="inbox"
        onValueChange={vi.fn()}
        options={[
          { value: `inbox` as const, label: `Inbox`, icon: conceptIcon(`nav-search`) },
          { value: `mine` as const, label: `My issues` },
        ]}
      />
    )
    const tabs = screen.getAllByRole(`tab`)
    expect(tabs[0]!.querySelector(`svg`)).toBeTruthy()
    expect(tabs[1]!.querySelector(`svg`)).toBeNull()
  })

  it(`disables one segment, or all of them`, () => {
    const { rerender } = render(
      <SegmentedControl
        value="inbox"
        onValueChange={vi.fn()}
        options={[OPTIONS[0]!, { ...OPTIONS[1]!, disabled: true }]}
      />
    )
    expect(screen.getAllByRole(`tab`)[1]!.hasAttribute(`disabled`)).toBe(true)
    expect(screen.getAllByRole(`tab`)[0]!.hasAttribute(`disabled`)).toBe(false)

    rerender(
      <SegmentedControl
        value="inbox"
        onValueChange={vi.fn()}
        options={OPTIONS}
        disabled
      />
    )
    for (const tab of screen.getAllByRole(`tab`)) {
      expect(tab.hasAttribute(`disabled`)).toBe(true)
    }
  })

  it(`becomes the group's first ROW when embedded`, () => {
    const { container } = render(
      <SegmentedControl
        value="inbox"
        onValueChange={vi.fn()}
        options={OPTIONS}
        embedded
      />
    )
    expect(container.querySelector(`[data-slot=glass-tabs-row]`)).toBeTruthy()
    expect(container.querySelector(`[data-slot=segmented-control]`)).toBeNull()
    // The embedded arm drops the floating capsule's own border and fill.
    const list = screen.getByRole(`tablist`)
    expect(list.className).toContain(`rounded-none`)
    expect(list.className).toContain(`bg-transparent`)
    expect(screen.getAllByRole(`tab`)).toHaveLength(2)
  })

  // EXP-1002: in a side panel the natural `w-fit` capsule runs past the column
  // and clips its last label. `fill` spans the container instead; the segments
  // are `flex-1` either way, so spanning is all it takes to share it.
  it(`spans its container when filled`, () => {
    const { rerender } = render(
      <SegmentedControl value="inbox" onValueChange={vi.fn()} options={OPTIONS} />
    )
    const natural = screen.getByRole(`tablist`).className
    expect(natural).toContain(`w-fit`)
    expect(natural).not.toContain(`w-full`)

    rerender(
      <SegmentedControl value="inbox" onValueChange={vi.fn()} options={OPTIONS} fill />
    )
    expect(screen.getByRole(`tablist`).className).toContain(`w-full`)
    for (const tab of screen.getAllByRole(`tab`)) {
      expect(tab.className).toContain(`flex-1`)
    }
  })
})
