import { readFileSync } from "node:fs"
import { join } from "node:path"
import { useRef } from "react"
import { act, fireEvent, render, screen, within } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { conceptIcon } from "./icons.generated"
import {
  ISSUE_MENU_LAYOUT,
  issueMenuEntries,
  issueMenuLabels,
  issueMenuSampleEntries,
} from "./issue-menu"
import {
  Menu,
  MenuGestureHost,
  MenuPanel,
  menuProps,
  tidyMenuEntries,
  useMenuGestures,
  type MenuEntry,
} from "./menu"
import { MENU_CHECK_ITEM_CLASS, MENU_ITEM_CLASS, MENU_SUB_TRIGGER_CLASS } from "./menu-surface"
import { PickerMenuRows } from "./picker/picker-menu-rows"

// UI cleanup batch — THE menu: one data-driven component, three
// presentations (a dropdown at a trigger, a context menu at the pointer, a
// bottom sheet on a phone), the pointer cursor on every row, and the
// document gesture host every opted-in element shares.

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

function setViewport(width: number) {
  Object.defineProperty(window, `innerWidth`, { configurable: true, value: width })
}

const openTrigger = (name: string) => {
  act(() => {
    fireEvent.pointerDown(screen.getByRole(`button`, { name }), {
      button: 0,
      ctrlKey: false,
      pointerType: `mouse`,
    })
  })
}

function entries(spies: {
  onCopy?: () => void
  onDelete?: () => void
  onToggle?: (next: boolean) => void
  onEffort?: (value: string) => void
} = {}): MenuEntry[] {
  return [
    { kind: `header`, identifier: `EXP-1`, title: `Ship it` },
    { kind: `item`, label: `Copy link`, icon: conceptIcon(`ui-copy`), shortcut: `⌘C`, onSelect: spies.onCopy ?? (() => {}) },
    { kind: `separator` },
    { kind: `separator` },
    {
      kind: `submenu`,
      label: `Effort`,
      value: `High`,
      body: (
        <PickerMenuRows
          mode="single"
          items={[
            { value: `low`, label: `Low` },
            { value: `high`, label: `High` },
          ]}
          value="high"
          onChange={spies.onEffort ?? (() => {})}
        />
      ),
    },
    { kind: `toggle`, label: `Ultracode`, checked: false, onChange: spies.onToggle ?? (() => {}) },
    { kind: `item`, label: `Delete`, destructive: true, onSelect: spies.onDelete ?? (() => {}) },
    { kind: `separator` },
  ]
}

beforeEach(() => setViewport(1024))
afterEach(() => setViewport(1024))

describe(`the row recipe`, () => {
  it(`gives every menu row the pointer cursor, a disabled one the arrow`, () => {
    for (const recipe of [MENU_ITEM_CLASS, MENU_SUB_TRIGGER_CLASS, MENU_CHECK_ITEM_CLASS]) {
      expect(recipe).toContain(`cursor-pointer`)
      expect(recipe).toContain(`data-[disabled]:cursor-default`)
      expect(recipe).not.toMatch(/(^| )cursor-default/)
    }
  })

  it(`lists the check and radio roles in the base pointer rule`, () => {
    const css = readFileSync(join(__dirname, `styles.css`), `utf8`)
    const rule = css.slice(css.indexOf(`[role="menuitem"]`), css.indexOf(`cursor: pointer;`))
    expect(rule).toContain(`[role="menuitemcheckbox"]`)
    expect(rule).toContain(`[role="menuitemradio"]`)
  })
})

describe(`tidyMenuEntries`, () => {
  it(`drops leading, trailing and doubled separators`, () => {
    const kinds = tidyMenuEntries([
      { kind: `separator` },
      { kind: `item`, label: `A`, onSelect: () => {} },
      { kind: `separator` },
      { kind: `separator` },
      { kind: `item`, label: `B`, onSelect: () => {} },
      { kind: `separator` },
    ]).map((entry) => entry.kind)
    expect(kinds).toEqual([`item`, `separator`, `item`])
  })
})

describe(`Menu — trigger (pointer device)`, () => {
  it(`draws the rows from data, in order, and runs the verb`, () => {
    const onCopy = vi.fn()
    render(
      <Menu
        aria-label="Issue actions"
        entries={entries({ onCopy })}
        trigger={<button type="button">More</button>}
      />
    )
    openTrigger(`More`)
    const menu = screen.getByRole(`menu`)
    expect(within(menu).getByText(`EXP-1`)).toBeTruthy()
    // The doubled separator collapsed, the trailing one dropped.
    expect(menu.querySelectorAll(`[role=separator]`)).toHaveLength(1)
    const delRow = within(menu).getByText(`Delete`).closest(`[role=menuitem]`)!
    expect(delRow.getAttribute(`data-variant`)).toBe(`destructive`)
    expect(delRow.className).toContain(`cursor-pointer`)
    fireEvent.click(within(menu).getByText(`Copy link`))
    expect(onCopy).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole(`menu`)).toBeNull()
  })

  it(`a toggle row flips and keeps the menu open`, () => {
    const onToggle = vi.fn()
    render(<Menu entries={entries({ onToggle })} trigger={<button type="button">More</button>} />)
    openTrigger(`More`)
    const row = screen.getByRole(`menuitemcheckbox`)
    expect(row.getAttribute(`aria-checked`)).toBe(`false`)
    fireEvent.click(row)
    expect(onToggle).toHaveBeenCalledWith(true)
    expect(screen.getByRole(`menu`)).toBeTruthy()
  })

  it(`a submenu's picker body marks the value and reports a pick`, () => {
    const onEffort = vi.fn()
    render(<Menu entries={entries({ onEffort })} trigger={<button type="button">More</button>} />)
    openTrigger(`More`)
    fireEvent.click(screen.getByText(`Effort`))
    const sub = document.querySelector<HTMLElement>(`[data-slot=dropdown-menu-sub-content]`)!
    const radios = within(sub).getAllByRole(`menuitemradio`)
    expect(radios[1]!.getAttribute(`aria-checked`)).toBe(`true`)
    expect(radios[1]!.querySelector(`[data-selected-glyph=check]`)).toBeTruthy()
    fireEvent.click(within(sub).getByText(`Low`))
    expect(onEffort).toHaveBeenCalledWith(`low`)
  })

  it(`a choice row wears the check while it is the current one`, () => {
    render(
      <Menu
        entries={[
          { kind: `item`, label: `Claude`, checked: true, onSelect: () => {} },
          { kind: `item`, label: `Codex`, checked: false, onSelect: () => {} },
        ]}
        trigger={<button type="button">Agent</button>}
      />
    )
    openTrigger(`Agent`)
    const [claude, codex] = screen.getAllByRole(`menuitemradio`)
    expect(claude!.querySelector(`[data-selected-glyph=check]`)).toBeTruthy()
    expect(codex!.querySelector(`[data-selected-glyph]`)).toBeNull()
  })
})

describe(`Menu — keyboard into a body submenu`, () => {
  const openByKeyboard = (name: string) => {
    const trigger = screen.getByRole(`button`, { name })
    act(() => trigger.focus())
    act(() => {
      fireEvent.keyDown(trigger, { key: `Enter` })
    })
  }
  const openSubByKeyboard = (label: string) => {
    const subTrigger = screen.getByText(label).closest<HTMLElement>(`[role=menuitem]`)!
    act(() => subTrigger.focus())
    act(() => {
      fireEvent.keyDown(subTrigger, { key: `ArrowRight` })
    })
    return subTrigger
  }

  it(`→ on a searchable body's row lands focus in its search field`, () => {
    render(
      <Menu
        entries={[
          {
            kind: `submenu`,
            label: `Implement issue`,
            body: (
              <div>
                <input data-slot="command-input" aria-label="Search issues" />
                <div role="option" aria-selected="false">EXP-1</div>
              </div>
            ),
          },
        ]}
        trigger={<button type="button">Plus</button>}
      />
    )
    openByKeyboard(`Plus`)
    openSubByKeyboard(`Implement issue`)
    const field = screen.getByRole(`textbox`, { name: `Search issues` })
    expect(document.activeElement).toBe(field)
    // The caret keys stay the field's: ← does not close the submenu.
    act(() => {
      fireEvent.keyDown(field, { key: `ArrowLeft` })
    })
    expect(document.querySelector(`[data-slot=dropdown-menu-sub-content]`)).toBeTruthy()
    expect(document.activeElement).toBe(field)
  })

  it(`← from a picker row returns to the sub-trigger`, () => {
    render(<Menu entries={entries()} trigger={<button type="button">More</button>} />)
    openByKeyboard(`More`)
    const subTrigger = openSubByKeyboard(`Effort`)
    const sub = document.querySelector<HTMLElement>(`[data-slot=dropdown-menu-sub-content]`)!
    const row = within(sub).getAllByRole(`menuitemradio`)[0]!
    act(() => row.focus())
    act(() => {
      fireEvent.keyDown(row, { key: `ArrowLeft` })
    })
    expect(document.querySelector(`[data-slot=dropdown-menu-sub-content]`)).toBeNull()
    expect(document.activeElement).toBe(subTrigger)
  })
})

describe(`Menu — sheet (phones)`, () => {
  it(`becomes a bottom sheet below md, submenus as pages with a back row`, () => {
    setViewport(390)
    const onEffort = vi.fn()
    render(
      <Menu
        title="Composer"
        entries={entries({ onEffort })}
        trigger={<button type="button">Add</button>}
      />
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Add` }))
    const sheet = document.querySelector<HTMLElement>(`[data-slot=menu-sheet]`)!
    expect(sheet).toBeTruthy()
    expect(within(sheet).getByText(`Composer`)).toBeTruthy()
    fireEvent.click(within(sheet).getByText(`Effort`))
    expect(within(sheet).getByTestId(`menu-sheet-back`).textContent).toContain(`Effort`)
    expect(within(sheet).queryByText(`Copy link`)).toBeNull()
    fireEvent.click(within(sheet).getByText(`Low`))
    expect(onEffort).toHaveBeenCalledWith(`low`)
    // A single pick closes the whole sheet.
    expect(document.querySelector(`[data-slot=menu-sheet]`)).toBeNull()
  })

  it(`the back row returns to the top level`, () => {
    render(
      <Menu
        mode="sheet"
        open
        onOpenChange={() => {}}
        entries={entries()}
      />
    )
    const sheet = document.querySelector<HTMLElement>(`[data-slot=menu-sheet]`)!
    fireEvent.click(within(sheet).getByText(`Effort`))
    fireEvent.click(within(sheet).getByTestId(`menu-sheet-back`))
    expect(within(sheet).getByText(`Copy link`)).toBeTruthy()
  })
})

describe(`Menu — pointer`, () => {
  it(`hangs at a 0×0 anchor at the pointer`, () => {
    render(
      <Menu
        mode="pointer"
        anchor={{ x: 40, y: 50, width: 0, height: 0 }}
        open
        onOpenChange={() => {}}
        entries={entries()}
      />
    )
    const anchor = screen.getByTestId(`menu-pointer-anchor`)
    expect(anchor.style.left).toBe(`40px`)
    expect(anchor.style.top).toBe(`50px`)
    expect(screen.getByRole(`menu`)).toBeTruthy()
  })
})

describe(`MenuPanel`, () => {
  it(`draws the same rows at rest, in both looks`, () => {
    const { rerender } = render(<MenuPanel entries={entries()} />)
    const panel = document.querySelector(`[data-slot=menu-panel]`)!
    expect(panel.getAttribute(`data-look`)).toBe(`dropdown`)
    expect(panel.textContent).toContain(`Copy link`)
    expect(panel.querySelector(`[data-submenu]`)?.textContent).toContain(`High`)
    rerender(<MenuPanel entries={entries()} look="sheet" title="Issue" />)
    expect(document.querySelector(`[data-slot=menu-panel]`)!.getAttribute(`data-look`)).toBe(`sheet`)
  })
})

describe(`the issue menu layout`, () => {
  it(`renders only the rows the live menu filled, in the layout's order`, () => {
    const filled = issueMenuEntries({
      header: { identifier: `EXP-1`, title: `Ship it` },
      conditions: new Set([`estimation`]),
      slots: {
        open: { onSelect: () => {} },
        status: { value: `Backlog`, entries: [] },
        estimate: { value: `3`, entries: [] },
        "move-board": { value: `App`, entries: [] },
        delete: { onSelect: () => {} },
      },
    })
    const labels = filled
      .filter((entry) => entry.kind === `item` || entry.kind === `submenu`)
      .map((entry) => (entry as { label: string }).label)
    // move-board needs the `boards` condition, so it drops out.
    expect(labels).toEqual([`Open issue`, `Status`, `Estimate`, `Delete issue`])
    expect(filled[0]!.kind).toBe(`header`)
  })

  it(`the samples cover every row of the layout`, () => {
    const all = issueMenuSampleEntries(new Set([`phone`, `duplicate`, `estimation`, `boards`]), {
      identifier: `EXP-1`,
      title: `t`,
    })
    const rows = all.filter((entry) => entry.kind === `item` || entry.kind === `submenu`)
    expect(rows).toHaveLength(ISSUE_MENU_LAYOUT.filter((row) => row.kind !== `separator`).length)
    expect(issueMenuLabels(new Set())).not.toContain(`Select`)
  })
})

describe(`the gesture host`, () => {
  function Host({ onOpen, onRowClick }: { onOpen: () => void; onRowClick: () => void }) {
    const openRef = useRef(false)
    useMenuGestures(onOpen, openRef, (kind) => kind === `issue`)
    return (
      <>
        <div data-testid="row" onClick={onRowClick} {...menuProps(`issue`, `i1`)}>
          <span data-testid="cell">APP-1</span>
        </div>
        <div data-testid="tab" {...menuProps(`work-tab`, `t1`)}>
          tab
        </div>
      </>
    )
  }

  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it(`swallows the click that follows a contextmenu on the same element, once`, () => {
    const onOpen = vi.fn()
    const onRowClick = vi.fn()
    render(<Host onOpen={onOpen} onRowClick={onRowClick} />)
    fireEvent.contextMenu(screen.getByTestId(`cell`), { clientX: 10, clientY: 10 })
    expect(onOpen).toHaveBeenCalledTimes(1)
    fireEvent.click(screen.getByTestId(`cell`))
    expect(onRowClick).not.toHaveBeenCalled()
    fireEvent.click(screen.getByTestId(`row`))
    expect(onRowClick).toHaveBeenCalledTimes(1)
  })

  it(`lets a click through once the tail window has passed`, () => {
    const onOpen = vi.fn()
    const onRowClick = vi.fn()
    render(<Host onOpen={onOpen} onRowClick={onRowClick} />)
    fireEvent.contextMenu(screen.getByTestId(`row`), { clientX: 10, clientY: 10 })
    vi.advanceTimersByTime(150)
    fireEvent.click(screen.getByTestId(`row`))
    expect(onRowClick).toHaveBeenCalledTimes(1)
  })

  it(`leaves a kind it does not answer for to the browser`, () => {
    const onOpen = vi.fn()
    render(<Host onOpen={onOpen} onRowClick={() => {}} />)
    const untouched = fireEvent.contextMenu(screen.getByTestId(`tab`), { clientX: 5, clientY: 5 })
    expect(untouched).toBe(true)
    expect(onOpen).not.toHaveBeenCalled()
  })

  it(`MenuGestureHost renders the kind's menu at the pointer`, () => {
    render(
      <MenuGestureHost
        menus={{
          "work-tab": ({ target, open, onOpenChange }) => (
            <Menu
              mode="pointer"
              anchor={target.anchor}
              open={open}
              onOpenChange={onOpenChange}
              entries={[{ kind: `item`, label: `Close ${target.id}`, onSelect: () => {} }]}
            />
          ),
        }}
      >
        <div data-testid="tab" {...menuProps(`work-tab`, `t1`)}>
          tab
        </div>
      </MenuGestureHost>
    )
    expect(fireEvent.contextMenu(screen.getByTestId(`tab`), { clientX: 5, clientY: 5 })).toBe(false)
    expect(screen.getByText(`Close t1`)).toBeTruthy()
  })

  // A kind named like an Object built-in is not a menu the host answers for.
  it(`MenuGestureHost ignores a kind that only matches an Object built-in`, () => {
    render(
      <MenuGestureHost menus={{}}>
        <div data-testid="proto" {...menuProps(`toString`, `x`)}>
          x
        </div>
      </MenuGestureHost>
    )
    expect(fireEvent.contextMenu(screen.getByTestId(`proto`), { clientX: 5, clientY: 5 })).toBe(true)
    expect(screen.queryByRole(`menu`)).toBeNull()
  })
})
