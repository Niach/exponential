import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { ComboboxMenuItems } from "./combobox-menu"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuTrigger,
} from "./context-menu"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "./dropdown-menu"
import type { PickerOption } from "./picker-option"

// Radix positions menus with ResizeObserver, which jsdom lacks.
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

const openDropdown = () => {
  act(() => {
    fireEvent.pointerDown(screen.getByText(`Open`), {
      button: 0,
      ctrlKey: false,
      pointerType: `mouse`,
    })
  })
}

const rows = (slot: string) =>
  Array.from(document.querySelectorAll(`[data-slot=${slot}]`))

const glyphs = (kind: string) =>
  Array.from(document.querySelectorAll(`[data-selected-glyph="${kind}"]`))

describe(`ComboboxMenuItems — dropdown host`, () => {
  it(`renders the host's items with the single-select trailing check`, () => {
    render(
      <DropdownMenu>
        <DropdownMenuTrigger>Open</DropdownMenuTrigger>
        <DropdownMenuContent>
          <ComboboxMenuItems
            menu="dropdown"
            options={OPTIONS}
            value="b"
            onChange={vi.fn()}
          />
        </DropdownMenuContent>
      </DropdownMenu>
    )
    openDropdown()
    const items = rows(`dropdown-menu-item`)
    expect(items).toHaveLength(3)
    // The ONE single-select idiom — never the radio dot the menu ships.
    expect(glyphs(`check`)).toHaveLength(1)
    expect(items[1]!.querySelector(`[data-selected-glyph=check]`)).toBeTruthy()
    expect(items[1]!.getAttribute(`role`)).toBe(`menuitemradio`)
    expect(items[1]!.getAttribute(`aria-checked`)).toBe(`true`)
    expect(items[0]!.getAttribute(`aria-checked`)).toBe(`false`)
    expect(document.querySelector(`[data-slot=dropdown-menu-radio-item]`)).toBeNull()
    expect(document.querySelector(`[data-slot=dropdown-menu-checkbox-item]`)).toBeNull()
  })

  it(`reports a single pick and lets the menu close`, () => {
    const onChange = vi.fn()
    render(
      <DropdownMenu>
        <DropdownMenuTrigger>Open</DropdownMenuTrigger>
        <DropdownMenuContent>
          <ComboboxMenuItems
            menu="dropdown"
            options={OPTIONS}
            value={null}
            onChange={onChange}
          />
        </DropdownMenuContent>
      </DropdownMenu>
    )
    openDropdown()
    act(() => {
      fireEvent.click(rows(`dropdown-menu-item`)[2]!)
    })
    expect(onChange).toHaveBeenCalledWith(`c`)
    expect(rows(`dropdown-menu-item`)).toHaveLength(0)
  })

  it(`the none row reports null, and a mixed value marks nothing at all`, () => {
    const onChange = vi.fn()
    const { rerender } = render(
      <DropdownMenu>
        <DropdownMenuTrigger>Open</DropdownMenuTrigger>
        <DropdownMenuContent>
          <ComboboxMenuItems
            menu="dropdown"
            options={OPTIONS}
            value={null}
            onChange={onChange}
            noneLabel="Unassigned"
          />
        </DropdownMenuContent>
      </DropdownMenu>
    )
    openDropdown()
    const items = rows(`dropdown-menu-item`)
    expect(items).toHaveLength(4)
    expect(items[0]!.getAttribute(`data-combobox-none`)).toBe(`true`)
    expect(items[0]!.querySelector(`[data-selected-glyph=check]`)).toBeTruthy()
    for (const row of items) {
      expect(row.outerHTML).not.toContain(`__`)
    }

    rerender(
      <DropdownMenu>
        <DropdownMenuTrigger>Open</DropdownMenuTrigger>
        <DropdownMenuContent>
          <ComboboxMenuItems
            menu="dropdown"
            options={OPTIONS}
            value={null}
            onChange={onChange}
            noneLabel="Unassigned"
            indeterminate
          />
        </DropdownMenuContent>
      </DropdownMenu>
    )
    // A bulk edit over issues that disagree: not even "Unassigned" is current.
    expect(glyphs(`check`)).toHaveLength(0)

    act(() => {
      fireEvent.click(rows(`dropdown-menu-item`)[0]!)
    })
    expect(onChange).toHaveBeenCalledWith(null)
  })

  it(`multi draws the circle pair, honours checked, and stays open`, () => {
    const onChange = vi.fn()
    render(
      <DropdownMenu>
        <DropdownMenuTrigger>Open</DropdownMenuTrigger>
        <DropdownMenuContent>
          <ComboboxMenuItems
            menu="dropdown"
            multiple
            options={[
              { value: `a`, label: `Alpha` },
              { value: `b`, label: `Beta`, checked: `indeterminate` },
              { value: `c`, label: `Gamma` },
            ]}
            value={[`a`]}
            onChange={onChange}
          />
        </DropdownMenuContent>
      </DropdownMenu>
    )
    openDropdown()
    const items = rows(`dropdown-menu-item`)
    expect(glyphs(`selected`)).toHaveLength(1)
    expect(glyphs(`indeterminate`)).toHaveLength(1)
    expect(glyphs(`unselected`)).toHaveLength(1)
    expect(glyphs(`check`)).toHaveLength(0)
    expect(items[0]!.getAttribute(`role`)).toBe(`menuitemcheckbox`)
    expect(items[0]!.getAttribute(`aria-checked`)).toBe(`true`)
    expect(items[1]!.getAttribute(`aria-checked`)).toBe(`mixed`)
    expect(items[2]!.getAttribute(`aria-checked`)).toBe(`false`)

    // An indeterminate row JOINS on pick: "on some" becomes "on all".
    act(() => {
      fireEvent.click(items[1]!)
    })
    expect(onChange).toHaveBeenCalledWith([`a`, `b`])
    // A batch is several picks: the menu must survive one.
    expect(rows(`dropdown-menu-item`)).toHaveLength(3)

    act(() => {
      fireEvent.click(rows(`dropdown-menu-item`)[0]!)
    })
    expect(onChange).toHaveBeenCalledWith([])
  })

  it(`shows emptyText as one disabled row when there is nothing to pick`, () => {
    render(
      <DropdownMenu>
        <DropdownMenuTrigger>Open</DropdownMenuTrigger>
        <DropdownMenuContent>
          <ComboboxMenuItems
            menu="dropdown"
            multiple
            options={[]}
            value={[]}
            onChange={vi.fn()}
            emptyText="No labels yet"
          />
        </DropdownMenuContent>
      </DropdownMenu>
    )
    openDropdown()
    const items = rows(`dropdown-menu-item`)
    expect(items).toHaveLength(1)
    expect(items[0]!.textContent).toBe(`No labels yet`)
    expect(items[0]!.getAttribute(`data-disabled`)).toBe(``)
  })
})

describe(`ComboboxMenuItems — context-menu host`, () => {
  it(`renders the context menu's own items and reports a pick`, () => {
    const onChange = vi.fn()
    render(
      <ContextMenu>
        <ContextMenuTrigger>Row</ContextMenuTrigger>
        <ContextMenuContent>
          <ComboboxMenuItems
            menu="context"
            options={OPTIONS}
            value="a"
            onChange={onChange}
            renderOption={(option) => <em>{option.label}!</em>}
          />
        </ContextMenuContent>
      </ContextMenu>
    )
    act(() => {
      fireEvent.contextMenu(screen.getByText(`Row`))
    })
    const items = rows(`context-menu-item`)
    expect(items).toHaveLength(3)
    expect(rows(`dropdown-menu-item`)).toHaveLength(0)
    // The custom body renders INSIDE the primitive's row: the glyph is still
    // the primitive's, beside it.
    expect(items[0]!.querySelector(`em`)?.textContent).toBe(`Alpha!`)
    expect(items[0]!.querySelector(`[data-selected-glyph=check]`)).toBeTruthy()
    act(() => {
      fireEvent.click(items[2]!)
    })
    expect(onChange).toHaveBeenCalledWith(`c`)
  })
})
