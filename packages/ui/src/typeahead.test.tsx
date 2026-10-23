import { act, fireEvent, render, renderHook, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import {
  TYPEAHEAD_MENU_WIDTH,
  TYPEAHEAD_PORTAL_SELECTOR,
  TYPEAHEAD_ROW_CLASS,
  TypeaheadMenu,
  TypeaheadRow,
  placeTypeaheadMenu,
  useTypeahead,
} from "./typeahead"

// EXP-941: three floating menus each re-implemented these rules. The one that
// matters most is the LAST one — a menu that swallows Cmd+Enter eats the
// composer's send, which is a bug a user cannot work around.

const ITEMS = [`alpha`, `beta`, `gamma`]

const setup = (
  items: readonly string[] = ITEMS,
  onDismiss?: () => void,
  resetKey?: unknown
) => {
  const onAccept = vi.fn()
  const hook = renderHook(
    (props: { items: readonly string[]; resetKey?: unknown }) =>
      useTypeahead({ items: props.items, onAccept, onDismiss, resetKey: props.resetKey }),
    { initialProps: { items, resetKey } }
  )
  return { hook, onAccept }
}

const press = (
  hook: ReturnType<typeof setup>[`hook`],
  key: string,
  modifiers: Record<string, boolean> = {}
) => {
  const preventDefault = vi.fn()
  let handled = false
  act(() => {
    handled = hook.result.current.handleKeyDown({
      key,
      preventDefault,
      ...modifiers,
    })
  })
  return { handled, preventDefault }
}

describe(`useTypeahead`, () => {
  it(`starts on the first row`, () => {
    const { hook } = setup()
    expect(hook.result.current.active).toBe(0)
  })

  it(`wraps downwards and upwards`, () => {
    const { hook } = setup()
    expect(press(hook, `ArrowDown`).handled).toBe(true)
    expect(hook.result.current.active).toBe(1)
    press(hook, `ArrowDown`)
    press(hook, `ArrowDown`)
    expect(hook.result.current.active).toBe(0)
    const up = press(hook, `ArrowUp`)
    expect(up.handled).toBe(true)
    expect(up.preventDefault).toHaveBeenCalled()
    expect(hook.result.current.active).toBe(2)
  })

  it(`accepts on a plain Enter and on Tab`, () => {
    const { hook, onAccept } = setup()
    press(hook, `ArrowDown`)
    const enter = press(hook, `Enter`)
    expect(enter.handled).toBe(true)
    expect(enter.preventDefault).toHaveBeenCalled()
    expect(onAccept).toHaveBeenCalledWith(`beta`, 1)

    const tab = press(hook, `Tab`)
    expect(tab.handled).toBe(true)
    expect(onAccept).toHaveBeenCalledTimes(2)
  })

  it(`leaves the send shortcut completely alone`, () => {
    const { hook, onAccept } = setup()
    const modifiers: Record<string, boolean>[] = [
      { metaKey: true },
      { ctrlKey: true },
    ]
    for (const modifier of modifiers) {
      const result = press(hook, `Enter`, modifier)
      expect(result.handled).toBe(false)
      expect(result.preventDefault).not.toHaveBeenCalled()
    }
    expect(onAccept).not.toHaveBeenCalled()
  })

  it(`dismisses on Escape`, () => {
    const onDismiss = vi.fn()
    const { hook } = setup(ITEMS, onDismiss)
    const result = press(hook, `Escape`)
    expect(result.handled).toBe(true)
    expect(result.preventDefault).toHaveBeenCalled()
    expect(onDismiss).toHaveBeenCalledTimes(1)
  })

  it(`handles nothing at all with an empty list`, () => {
    const { hook, onAccept } = setup([])
    for (const key of [`ArrowDown`, `ArrowUp`, `Enter`, `Tab`, `Escape`]) {
      const result = press(hook, key)
      expect(result.handled, key).toBe(false)
      expect(result.preventDefault, key).not.toHaveBeenCalled()
    }
    expect(onAccept).not.toHaveBeenCalled()
  })

  it(`leaves a key mid IME composition to the IME`, () => {
    const { hook, onAccept } = setup()
    for (const key of [`Enter`, `Tab`, `ArrowDown`, `ArrowUp`, `Escape`]) {
      const result = press(hook, key, { isComposing: true })
      expect(result.handled, key).toBe(false)
      expect(result.preventDefault, key).not.toHaveBeenCalled()
    }
    expect(onAccept).not.toHaveBeenCalled()
    expect(hook.result.current.active).toBe(0)
  })

  it(`ignores a key it does not own`, () => {
    const { hook } = setup()
    const result = press(hook, `a`)
    expect(result.handled).toBe(false)
    expect(result.preventDefault).not.toHaveBeenCalled()
  })

  it(`goes back to the top when the query changes`, () => {
    const { hook } = setup(ITEMS, undefined, `al`)
    press(hook, `ArrowDown`)
    expect(hook.result.current.active).toBe(1)
    hook.rerender({ items: ITEMS, resetKey: `alp` })
    expect(hook.result.current.active).toBe(0)
  })

  it(`clamps the active row when the list shrinks`, () => {
    const { hook, onAccept } = setup()
    press(hook, `ArrowUp`)
    expect(hook.result.current.active).toBe(2)
    hook.rerender({ items: [`alpha`], resetKey: undefined })
    expect(hook.result.current.active).toBe(0)
    act(() => hook.result.current.acceptActive())
    expect(onAccept).toHaveBeenCalledWith(`alpha`, 0)
  })

  it(`accepts the active row on demand`, () => {
    const { hook, onAccept } = setup()
    act(() => hook.result.current.setActive(2))
    act(() => hook.result.current.acceptActive())
    expect(onAccept).toHaveBeenCalledWith(`gamma`, 2)
  })
})

describe(`TypeaheadMenu`, () => {
  it(`floats below by default and above on request`, () => {
    const { container, rerender } = render(<TypeaheadMenu>rows</TypeaheadMenu>)
    const menu = container.querySelector(`[data-slot=typeahead-menu]`)!
    expect(menu.getAttribute(`role`)).toBe(`listbox`)
    expect(menu.className).toContain(`top-full`)
    expect(menu.className).toContain(`mt-1`)
    expect(menu.className).toContain(`w-72`)
    expect(menu.className).toContain(`absolute`)

    rerender(<TypeaheadMenu placement="above">rows</TypeaheadMenu>)
    const above = container.querySelector(`[data-slot=typeahead-menu]`)!
    expect(above.className).toContain(`bottom-full`)
    expect(above.className).toContain(`mb-1`)
    expect(above.getAttribute(`data-placement`)).toBe(`above`)
  })

  it(`caps its height with the room the host measured`, () => {
    const { container } = render(
      <TypeaheadMenu maxHeight={240}>rows</TypeaheadMenu>
    )
    const menu = container.querySelector<HTMLElement>(
      `[data-slot=typeahead-menu]`
    )!
    expect(menu.style.maxHeight).toBe(`240px`)
  })
})

// EXP-959: the editor's arm. The caret rect goes in; the menu comes out
// fixed, flipped when it must, and outside the host's subtree.

const VIEWPORT = { top: 0, height: 800, width: 1200, innerHeight: 800 }
const caret = (top: number, left = 100) => ({ top, bottom: top + 20, left })

describe(`placeTypeaheadMenu`, () => {
  it(`hangs below the caret with a gap and the room below, capped`, () => {
    const placed = placeTypeaheadMenu(caret(100), VIEWPORT)
    expect(placed.placement).toBe(`below`)
    expect(placed.style).toEqual({ left: 100, top: 124, maxHeight: 320 })
  })

  it(`flips above when the room below is short and above has more`, () => {
    const placed = placeTypeaheadMenu(caret(700), VIEWPORT)
    expect(placed.placement).toBe(`above`)
    // bottom is measured against innerHeight; maxHeight = 700 - 8 - 4.
    expect(placed.style).toEqual({ left: 100, bottom: 104, maxHeight: 320 })
    // A shorter window: 192px below the caret, 252px above → above, capped
    // to the room there minus the gap.
    const tight = placeTypeaheadMenu(caret(260), { ...VIEWPORT, height: 480 })
    expect(tight.placement).toBe(`above`)
    expect(tight.style.maxHeight).toBe(248)
  })

  it(`stays below when neither side has room, capped to the room below`, () => {
    // 60px of room on either side: below wins the tie, and the cap never
    // goes under the minimum.
    const placed = placeTypeaheadMenu(caret(30), {
      ...VIEWPORT,
      height: 110,
    })
    expect(placed.placement).toBe(`below`)
    expect(placed.style.maxHeight).toBe(48)
  })

  it(`measures against the visual viewport, not the window`, () => {
    // The keyboard is up: the visible band is 320px tall starting at 0. A
    // caret 200px down has 100px below and 192px above, so it flips.
    const placed = placeTypeaheadMenu(caret(200), { ...VIEWPORT, height: 320 })
    expect(placed.placement).toBe(`above`)
    expect(placed.style.bottom).toBe(604)
    expect(placed.style.maxHeight).toBe(188)
  })

  it(`clamps horizontally to the viewport`, () => {
    expect(placeTypeaheadMenu(caret(100, 2), VIEWPORT).style.left).toBe(8)
    expect(placeTypeaheadMenu(caret(100, 1180), VIEWPORT).style.left).toBe(
      1200 - TYPEAHEAD_MENU_WIDTH - 8
    )
  })
})

describe(`TypeaheadMenu anchored`, () => {
  it(`portals to the body, fixed, above a dialog, and stamps the contract`, () => {
    const { container } = render(
      <div data-host>
        <TypeaheadMenu anchor={caret(100)}>rows</TypeaheadMenu>
      </div>
    )
    expect(container.querySelector(`[data-slot=typeahead-menu]`)).toBeNull()
    const menu = document.body.querySelector<HTMLElement>(
      `[data-slot=typeahead-menu]`
    )!
    expect(menu.parentElement).toBe(document.body)
    expect(menu.matches(TYPEAHEAD_PORTAL_SELECTOR)).toBe(true)
    expect(menu.getAttribute(`role`)).toBe(`listbox`)
    expect(menu.getAttribute(`data-placement`)).toBe(`below`)
    expect(menu.className).toContain(`fixed`)
    // The surface recipe's own z-50 must lose to the dialog-clearing z-[60].
    expect(menu.className).toContain(`z-[60]`)
    expect(menu.className).not.toContain(`z-50`)
    expect(menu.className).toContain(`pointer-events-auto`)
    expect(menu.className).not.toContain(`absolute`)
    expect(menu.style.position).toBe(``)
    expect(menu.style.left).toBe(`100px`)
    expect(menu.style.top).toBe(`124px`)
    expect(menu.style.maxHeight).not.toBe(``)
  })

  it(`lets an explicit maxHeight win over the measured one`, () => {
    render(
      <TypeaheadMenu anchor={caret(100)} maxHeight={120}>
        rows
      </TypeaheadMenu>
    )
    expect(
      document.body.querySelector<HTMLElement>(`[data-slot=typeahead-menu]`)!
        .style.maxHeight
    ).toBe(`120px`)
  })

  it(`reports a lost anchor on an outside scroll or a resize, not on its own`, () => {
    const onAnchorLost = vi.fn()
    const ref = { current: null as HTMLDivElement | null }
    render(
      <TypeaheadMenu anchor={caret(100)} onAnchorLost={onAnchorLost} ref={ref}>
        <div data-inner>rows</div>
      </TypeaheadMenu>
    )
    expect(ref.current?.matches(TYPEAHEAD_PORTAL_SELECTOR)).toBe(true)
    fireEvent.scroll(ref.current!.querySelector(`[data-inner]`)!)
    fireEvent.scroll(ref.current!)
    expect(onAnchorLost).not.toHaveBeenCalled()
    fireEvent.scroll(document)
    expect(onAnchorLost).toHaveBeenCalledTimes(1)
    fireEvent(window, new Event(`resize`))
    expect(onAnchorLost).toHaveBeenCalledTimes(2)
  })

  it(`stops listening once unmounted`, () => {
    const onAnchorLost = vi.fn()
    const { unmount } = render(
      <TypeaheadMenu anchor={caret(100)} onAnchorLost={onAnchorLost}>
        rows
      </TypeaheadMenu>
    )
    unmount()
    expect(document.body.querySelector(`[data-slot=typeahead-menu]`)).toBeNull()
    fireEvent.scroll(document)
    expect(onAnchorLost).not.toHaveBeenCalled()
  })
})

describe(`TypeaheadRow`, () => {
  it(`is an option button carrying the shared row recipe`, () => {
    render(<TypeaheadRow>alpha</TypeaheadRow>)
    const row = screen.getByRole(`option`)
    expect(row.tagName).toBe(`BUTTON`)
    expect(row.getAttribute(`type`)).toBe(`button`)
    expect(row.getAttribute(`aria-selected`)).toBe(`false`)
    expect(row.className).toContain(TYPEAHEAD_ROW_CLASS)
  })

  it(`paints the active row`, () => {
    render(<TypeaheadRow active>alpha</TypeaheadRow>)
    const row = screen.getByRole(`option`)
    expect(row.getAttribute(`aria-selected`)).toBe(`true`)
    expect(row.className).toContain(`bg-glass-active`)
  })

  it(`keeps the host's caret: mousedown is prevented, the click selects`, () => {
    const onSelect = vi.fn()
    render(<TypeaheadRow onSelect={onSelect}>alpha</TypeaheadRow>)
    const row = screen.getByRole(`option`)
    const prevented = !fireEvent.mouseDown(row)
    expect(prevented).toBe(true)
    expect(onSelect).not.toHaveBeenCalled()
    fireEvent.click(row)
    expect(onSelect).toHaveBeenCalledTimes(1)
  })

  it(`still forwards a hover handler`, () => {
    const onMouseEnter = vi.fn()
    render(<TypeaheadRow onMouseEnter={onMouseEnter}>alpha</TypeaheadRow>)
    fireEvent.mouseEnter(screen.getByRole(`option`))
    expect(onMouseEnter).toHaveBeenCalled()
  })
})
