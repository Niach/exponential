// FEED-131: the Solid SegmentedControl (the app's segmented-control.test.tsx
// on Solid).
import { describe, expect, it, vi } from "vitest"
import { Search } from "lucide-solid"
import { fireEvent, render, screen } from "@solidjs/testing-library"
import { SEGMENTED_LIST, SEGMENTED_TAB } from "./tabs"
import { SegmentedControl } from "./segmented-control"

const OPTIONS = [
  { value: `inbox` as const, label: `Inbox` },
  { value: `mine` as const, label: `My issues` },
]

describe(`SegmentedControl`, () => {
  it(`renders one tab per option and marks the active one`, () => {
    render(() => <SegmentedControl value="mine" onValueChange={vi.fn()} options={OPTIONS} />)
    const tabs = screen.getAllByRole(`tab`)
    expect(tabs.map((tab) => tab.textContent)).toEqual([`Inbox`, `My issues`])
    expect(tabs[1]!.getAttribute(`data-state`)).toBe(`active`)
    expect(tabs[0]!.getAttribute(`data-state`)).toBe(`inactive`)
    expect(document.querySelector(`[data-slot="segmented-control"]`)).toBeTruthy()
  })

  it(`reports the segment that was pressed`, () => {
    const onValueChange = vi.fn()
    render(() => <SegmentedControl value="inbox" onValueChange={onValueChange} options={OPTIONS} />)
    fireEvent.mouseDown(screen.getByRole(`tab`, { name: `My issues` }), { button: 0, ctrlKey: false })
    expect(onValueChange).toHaveBeenCalledWith(`mine`)
  })

  it(`uses the shared recipes; fill spans the container`, () => {
    render(() => <SegmentedControl value="inbox" onValueChange={vi.fn()} options={OPTIONS} fill />)
    expect(screen.getByRole(`tablist`).className).toContain(SEGMENTED_LIST.replace(`w-fit `, ``).split(` `)[0]!)
    expect(screen.getByRole(`tablist`).className).toContain(`w-full`)
    expect(screen.getAllByRole(`tab`)[0]!.className).toContain(SEGMENTED_TAB)
  })

  it(`draws a glyph when an option brings one; disables one or all`, () => {
    render(() => <SegmentedControl value="inbox" onValueChange={vi.fn()} options={[{ ...OPTIONS[0]!, icon: Search }, { ...OPTIONS[1]!, disabled: true }]} />)
    const tabs = screen.getAllByRole(`tab`) as HTMLButtonElement[]
    expect(tabs[0]!.querySelector(`svg`)?.getAttribute(`aria-hidden`)).toBe(`true`)
    expect(tabs[1]!.querySelector(`svg`)).toBeNull()
    expect(tabs[1]!.disabled).toBe(true)
    expect(tabs[0]!.disabled).toBe(false)
  })
})
