import { act, fireEvent, render, renderHook, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import {
  TYPEAHEAD_ROW_CLASS,
  TypeaheadMenu,
  TypeaheadRow,
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
