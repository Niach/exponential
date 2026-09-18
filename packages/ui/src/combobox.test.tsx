import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { Combobox, ComboboxList } from "./combobox"
import type { PickerOption } from "./picker-option"

// Radix positions its content with ResizeObserver and cmdk scrolls the active
// row into view; jsdom has neither.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const OPTIONS: PickerOption<string>[] = [
  { value: `a`, label: `Alpha` },
  { value: `b`, label: `Beta` },
  { value: `c`, label: `Gamma` },
]

const open = (name: string | RegExp = /.*/) => {
  const trigger = screen.getAllByRole(`button`)[0]!
  fireEvent.click(trigger)
  void name
  return trigger
}

const rows = () =>
  Array.from(document.querySelectorAll(`[data-slot=command-item]`))

const glyphs = (kind: string) =>
  Array.from(document.querySelectorAll(`[data-selected-glyph="${kind}"]`))

describe(`Combobox — single`, () => {
  it(`renders one row per option and exactly one check`, () => {
    render(
      <Combobox
        options={OPTIONS}
        value="b"
        onChange={vi.fn()}
        mobileTitle="Pick one"
      />
    )
    open()
    expect(rows()).toHaveLength(3)
    // The single-select affordance is the TRAILING check, on the picked row
    // only — never a checkbox, never a leading circle.
    expect(glyphs(`check`)).toHaveLength(1)
    expect(glyphs(`selected`)).toHaveLength(0)
    expect(glyphs(`unselected`)).toHaveLength(0)
    expect(rows()[1]!.textContent).toContain(`Beta`)
    expect(rows()[1]!.querySelector(`[data-selected-glyph=check]`)).toBeTruthy()
  })

  it(`reports the pick and closes`, () => {
    const onChange = vi.fn()
    render(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={onChange}
        mobileTitle="Pick one"
      />
    )
    open()
    fireEvent.click(rows()[2]!)
    expect(onChange).toHaveBeenCalledWith(`c`)
    expect(rows()).toHaveLength(0)
  })

  it(`never fires onChange on mount`, () => {
    const onChange = vi.fn()
    render(
      <Combobox
        options={OPTIONS}
        value="a"
        onChange={onChange}
        mobileTitle="Pick one"
      />
    )
    open()
    expect(onChange).not.toHaveBeenCalled()
  })

  it(`retires the sentinel: noneLabel reports null and carries no fake value`, () => {
    const onChange = vi.fn()
    render(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={onChange}
        noneLabel="Unassign"
        mobileTitle="Assignee"
      />
    )
    open()
    const all = rows()
    expect(all).toHaveLength(4)
    expect(all[0]!.textContent).toContain(`Unassign`)
    // Nothing picked, so the check sits on the None row.
    expect(all[0]!.querySelector(`[data-selected-glyph=check]`)).toBeTruthy()
    // No `__none__`/`__unassign__` string reaches the DOM or a caller.
    for (const row of all) {
      expect(row.outerHTML).not.toContain(`__`)
    }
    fireEvent.click(all[0]!)
    expect(onChange).toHaveBeenCalledWith(null)
  })

  it(`indeterminate marks no row, not even the none row`, () => {
    render(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={vi.fn()}
        noneLabel="Unassign"
        indeterminate
        mobileTitle="Assignee"
      />
    )
    open()
    expect(rows()).toHaveLength(4)
    expect(glyphs(`check`)).toHaveLength(0)
  })
})

describe(`Combobox — multiple`, () => {
  it(`draws the circle pair on EVERY row and stays open`, () => {
    const onChange = vi.fn()
    render(
      <Combobox
        multiple
        options={OPTIONS}
        value={[`a`]}
        onChange={onChange}
        mobileTitle="Pick many"
      />
    )
    open()
    expect(glyphs(`selected`)).toHaveLength(1)
    expect(glyphs(`unselected`)).toHaveLength(2)
    expect(glyphs(`check`)).toHaveLength(0)
    expect(rows()[0]!.getAttribute(`aria-pressed`)).toBe(`true`)
    expect(rows()[1]!.getAttribute(`aria-pressed`)).toBe(`false`)

    fireEvent.click(rows()[1]!)
    expect(onChange).toHaveBeenCalledWith([`a`, `b`])
    // A batch is several picks: the list must survive one.
    expect(rows()).toHaveLength(3)
  })

  it(`toggles a picked row back off`, () => {
    const onChange = vi.fn()
    render(
      <Combobox
        multiple
        options={OPTIONS}
        value={[`a`, `b`]}
        onChange={onChange}
        mobileTitle="Pick many"
      />
    )
    open()
    fireEvent.click(rows()[0]!)
    expect(onChange).toHaveBeenCalledWith([`b`])
  })

  it(`a row's checked wins over the value array, and indeterminate joins on pick`, () => {
    const onChange = vi.fn()
    render(
      <Combobox
        multiple
        options={[
          { value: `a`, label: `Alpha`, checked: true },
          { value: `b`, label: `Beta`, checked: `indeterminate` },
          { value: `c`, label: `Gamma` },
        ]}
        value={[`a`]}
        onChange={onChange}
        mobileTitle="Labels"
      />
    )
    open()
    expect(glyphs(`selected`)).toHaveLength(1)
    expect(glyphs(`indeterminate`)).toHaveLength(1)
    expect(glyphs(`unselected`)).toHaveLength(1)
    expect(rows()[1]!.getAttribute(`aria-pressed`)).toBe(`mixed`)
    // "On some" reads as not-yet-picked: a pick puts it on ALL.
    fireEvent.click(rows()[1]!)
    expect(onChange).toHaveBeenCalledWith([`a`, `b`])
  })

  it(`disables the unselected rows at the cap, never the picked ones`, () => {
    const onChange = vi.fn()
    render(
      <Combobox
        multiple
        max={2}
        options={OPTIONS}
        value={[`a`, `b`]}
        onChange={onChange}
        mobileTitle="Pick many"
      />
    )
    open()
    const all = rows()
    expect(all[2]!.getAttribute(`data-disabled`)).toBe(`true`)
    expect(all[0]!.getAttribute(`data-disabled`)).not.toBe(`true`)
    fireEvent.click(all[0]!)
    expect(onChange).toHaveBeenCalledWith([`b`])
  })
})

describe(`Combobox — shell`, () => {
  it(`hides the search field when searchable is false`, () => {
    const { rerender } = render(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={vi.fn()}
        mobileTitle="Pick one"
      />
    )
    open()
    expect(document.querySelector(`[data-slot=command-input]`)).toBeTruthy()

    rerender(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={vi.fn()}
        searchable={false}
        mobileTitle="Pick one"
      />
    )
    expect(document.querySelector(`[data-slot=command-input]`)).toBeNull()
  })

  it(`hands ranking over: shouldFilter=false keeps the caller's order`, () => {
    const onQueryChange = vi.fn()
    // Deliberately NOT alphabetical — the caller ranked these.
    const ranked: PickerOption<string>[] = [
      { value: `c`, label: `Gamma` },
      { value: `a`, label: `Alpha` },
      { value: `b`, label: `Beta` },
    ]
    render(
      <Combobox
        options={ranked}
        value={null}
        onChange={vi.fn()}
        shouldFilter={false}
        query="gam"
        onQueryChange={onQueryChange}
        mobileTitle="Pick one"
      />
    )
    open()
    expect(rows().map((row) => row.textContent)).toEqual([
      `Gamma`,
      `Alpha`,
      `Beta`,
    ])
    const input = document.querySelector(
      `[data-slot=command-input]`
    ) as HTMLInputElement
    expect(input.value).toBe(`gam`)
    fireEvent.change(input, { target: { value: `gamm` } })
    expect(onQueryChange).toHaveBeenCalledWith(`gamm`)
  })

  it(`renders two options that share a label, and both are pickable`, () => {
    const onChange = vi.fn()
    render(
      <Combobox
        options={[
          { value: `board-1`, label: `Design` },
          { value: `board-2`, label: `Design` },
        ]}
        value={null}
        onChange={onChange}
        mobileTitle="Boards"
      />
    )
    open()
    expect(rows()).toHaveLength(2)
    fireEvent.click(rows()[1]!)
    expect(onChange).toHaveBeenCalledWith(`board-2`)
  })

  it(`puts the width literal on the desktop content`, () => {
    render(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={vi.fn()}
        width="md"
        mobileTitle="Pick one"
        data-testid="pick-one"
      />
    )
    open()
    const content = document.querySelector(`[data-slot=popover-content]`)!
    expect(content.className).toContain(`w-[16rem]`)
    expect(content.className).toContain(`p-0`)
    expect(screen.getByTestId(`pick-one`)).toBeTruthy()
  })

  it(`never opens while disabled`, () => {
    render(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={vi.fn()}
        disabled
        mobileTitle="Pick one"
      />
    )
    open()
    expect(rows()).toHaveLength(0)
  })

  it(`replaces search and list with a panel`, () => {
    render(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={vi.fn()}
        panel={<div>Create label</div>}
        mobileTitle="Labels"
      />
    )
    open()
    expect(screen.getByText(`Create label`)).toBeTruthy()
    expect(rows()).toHaveLength(0)
    expect(document.querySelector(`[data-slot=command-input]`)).toBeNull()
  })

  it(`renders the footer after the list`, () => {
    render(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={vi.fn()}
        footer={<button type="button">Create label</button>}
        mobileTitle="Labels"
      />
    )
    open()
    const list = document.querySelector(`[data-slot=command-list]`)!
    const footer = document.querySelector(`[data-slot=combobox-footer]`)!
    expect(footer.textContent).toBe(`Create label`)
    expect(
      list.compareDocumentPosition(footer) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy()
  })

  it(`shows loading and error rows instead of the list`, () => {
    const { rerender } = render(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={vi.fn()}
        loading
        mobileTitle="Branch"
      />
    )
    open()
    expect(screen.getByText(`Loading…`)).toBeTruthy()
    expect(rows()).toHaveLength(0)

    rerender(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={vi.fn()}
        error="Couldn’t load branches"
        mobileTitle="Branch"
      />
    )
    expect(screen.getByText(`Couldn’t load branches`)).toBeTruthy()
    expect(rows()).toHaveLength(0)
  })

  it(`renders a bespoke trigger with the summary`, () => {
    render(
      <Combobox
        options={OPTIONS}
        value="a"
        onChange={vi.fn()}
        mobileTitle="Pick one"
        renderTrigger={({ summary, selected }) => (
          <button type="button">{`${summary} (${selected.length})`}</button>
        )}
      />
    )
    expect(screen.getByRole(`button`).textContent).toBe(`Alpha (1)`)
  })

  it(`summarises a multi pick on the field trigger`, () => {
    render(
      <Combobox
        multiple
        options={OPTIONS}
        value={[`a`, `b`, `c`]}
        onChange={vi.fn()}
        triggerVariant="field"
        triggerLabel="Any label"
        mobileTitle="Labels"
      />
    )
    expect(screen.getByRole(`button`).textContent).toContain(`Alpha, Beta +1`)
  })

  it(`falls back to the placeholder label while nothing is picked`, () => {
    render(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={vi.fn()}
        triggerLabel="Assignee"
        mobileTitle="Assignee"
      />
    )
    expect(screen.getByRole(`button`).textContent).toContain(`Assignee`)
  })

  it(`drops the trigger entirely when the host owns the open state`, () => {
    const { rerender } = render(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={vi.fn()}
        hideTrigger
        open={false}
        onOpenChange={vi.fn()}
        mobileTitle="Move to board"
      />
    )
    expect(screen.queryAllByRole(`button`)).toHaveLength(0)
    rerender(
      <Combobox
        options={OPTIONS}
        value={null}
        onChange={vi.fn()}
        hideTrigger
        open
        onOpenChange={vi.fn()}
        mobileTitle="Move to board"
      />
    )
    expect(rows()).toHaveLength(3)
  })
})

describe(`ComboboxList`, () => {
  it(`renders standalone, with no popover around it`, () => {
    const onChange = vi.fn()
    render(
      <ComboboxList options={OPTIONS} value="a" onChange={onChange} />
    )
    expect(document.querySelector(`[data-slot=popover-content]`)).toBeNull()
    expect(rows()).toHaveLength(3)
    expect(glyphs(`check`)).toHaveLength(1)
    fireEvent.click(rows()[1]!)
    expect(onChange).toHaveBeenCalledWith(`b`)
  })

  it(`renders the option's icon, dot and hint`, () => {
    render(
      <ComboboxList
        options={[
          { value: `main`, label: `main`, hint: `default`, dot: `#ef4444` },
        ]}
        value={null}
        onChange={vi.fn()}
      />
    )
    const row = rows()[0]!
    expect(row.textContent).toContain(`default`)
    expect(row.querySelector(`span[style*="background-color"]`)).toBeTruthy()
  })

  it(`lets renderOption replace the body but not the glyph`, () => {
    render(
      <ComboboxList
        multiple
        options={OPTIONS}
        value={[`a`]}
        onChange={vi.fn()}
        renderOption={(option, { selected }) => (
          <span>{`${option.value}:${selected}`}</span>
        )}
      />
    )
    expect(rows()[0]!.textContent).toBe(`a:true`)
    expect(glyphs(`selected`)).toHaveLength(1)
    expect(glyphs(`unselected`)).toHaveLength(2)
  })
})
