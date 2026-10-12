import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import spec from "@exp/domain-contract/fixtures/search-list.json"

import { SearchList, typeaheadPrefill } from "./search-list"
import type { PickerItem } from "./picker/picker-item"

// SLOP-28 — the ONE search list, replayed off search-list.json: the `#`
// prefill cases every client runs by name, then the list's own keys.

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

describe(`typeaheadPrefill`, () => {
  for (const testCase of spec.typeaheadPrefill) {
    it(testCase.name, () => {
      expect(typeaheadPrefill(testCase.text, testCase.caret)).toEqual(
        testCase.expected
      )
    })
  }
})

const ISSUES: PickerItem[] = Array.from({ length: 12 }, (_, index) => ({
  value: `t${index}`,
  label: `sidebar thing ${index}`,
  identifier: `EXP-${1240 + index}`,
  keywords: [`EXP-${1240 + index}`, `sidebar thing ${index}`],
}))

const rows = () => screen.getAllByTestId(spec.testIds.row)
const input = () => screen.getByTestId(spec.testIds.input)

describe(`SearchList`, () => {
  it(`draws every row (no cap) at the contract's pointer height, the identifier in its own column`, () => {
    render(
      <SearchList mode="single" value={null} onChange={vi.fn()} items={ISSUES} density="pointer" />
    )
    expect(rows()).toHaveLength(12)
    expect(rows()[0]!.className).toContain(`h-8`)
    expect(
      rows()[0]!.querySelector(`[data-slot=picker-identifier]`)?.textContent
    ).toBe(`EXP-1240`)
  })

  it(`caps the visible list at eight rows and fades the bottom while more sit below`, () => {
    render(
      <SearchList mode="single" value={null} onChange={vi.fn()} items={ISSUES} density="pointer" />
    )
    const list = document.querySelector<HTMLElement>(`[data-slot=command-list]`)!
    expect(list.style.maxHeight).toContain(`${spec.maxVisibleRows * spec.row.height}px`)
  })

  it(`preselects the top row; Enter and Tab pick the selected one`, () => {
    const onChange = vi.fn()
    render(
      <SearchList mode="single" value={null} onChange={onChange} items={ISSUES} density="pointer" />
    )
    expect(rows()[0]!.getAttribute(`aria-selected`)).toBe(`true`)
    fireEvent.keyDown(input(), { key: `ArrowDown` })
    fireEvent.keyDown(input(), { key: `Tab` })
    expect(onChange).toHaveBeenLastCalledWith(`t1`)
    fireEvent.keyDown(input(), { key: `Enter` })
    expect(onChange).toHaveBeenCalledTimes(2)
  })

  it(`shows the result count once a query is typed`, () => {
    render(
      <SearchList
        mode="single"
        value={null}
        onChange={vi.fn()}
        items={ISSUES}
        query="sid"
        shouldFilter={false}
        density="pointer"
      />
    )
    expect(screen.getByTestId(spec.testIds.count).textContent).toBe(`12`)
  })

  it(`reports Backspace in the EMPTY input only`, () => {
    const onBackspaceEmpty = vi.fn()
    const { rerender } = render(
      <SearchList
        mode="single"
        value={null}
        onChange={vi.fn()}
        items={ISSUES}
        query="s"
        onBackspaceEmpty={onBackspaceEmpty}
        density="pointer"
      />
    )
    fireEvent.keyDown(input(), { key: `Backspace` })
    expect(onBackspaceEmpty).not.toHaveBeenCalled()
    rerender(
      <SearchList
        mode="single"
        value={null}
        onChange={vi.fn()}
        items={ISSUES}
        query=""
        onBackspaceEmpty={onBackspaceEmpty}
        density="pointer"
      />
    )
    fireEvent.keyDown(input(), { key: `Backspace` })
    expect(onBackspaceEmpty).toHaveBeenCalledTimes(1)
  })

  it(`marks multi picks with the trailing check and toggles them`, () => {
    const onChange = vi.fn()
    render(
      <SearchList
        mode="multi"
        value={[`t0`]}
        onChange={onChange}
        items={ISSUES}
        density="pointer"
      />
    )
    expect(document.querySelectorAll(`[data-selected-glyph=check]`)).toHaveLength(1)
    fireEvent.click(rows()[1]!)
    expect(onChange).toHaveBeenCalledWith([`t0`, `t1`])
  })

  it(`takes the phone row height on a phone`, () => {
    render(
      <SearchList mode="single" value={null} onChange={vi.fn()} items={ISSUES} density="phone" />
    )
    expect(rows()[0]!.className).toContain(`h-11`)
  })

  it(`loading draws skeleton rows at the list's own row height, never the text`, () => {
    render(
      <SearchList
        mode="single"
        value={null}
        onChange={vi.fn()}
        items={ISSUES}
        density="phone"
        loading
      />
    )
    expect(screen.queryAllByTestId(spec.testIds.row)).toHaveLength(0)
    expect(screen.queryByText(`Loading…`)).toBeNull()
    const skeleton = screen.getByTestId(`feedback-skeleton`)
    const row = skeleton.querySelector<HTMLElement>(`[data-part=row]`)
    expect(row?.style.height).toBe(`${spec.row.heightPhone}px`)
  })
})
