// FEED-131: the cmdk-free Command: filtering + ordering, keyboard, onSelect,
// the empty state, group and separator hiding, a controlled query. The
// Solid twin (exponential-ui-solid/src/primitives/command.test.tsx) runs the
// same cases.

import { describe, expect, it, vi } from "vitest"
import { act, fireEvent, render } from "@testing-library/react"
import { useState } from "react"
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandSeparator } from "./command"

function Menu(props: { query?: string; onSelect?: (v: string) => void; loop?: boolean }) {
  return (
    <Command loop={props.loop} label="Commands">
      <CommandInput defaultValue={props.query} placeholder="Search" />
      <CommandList>
        <CommandEmpty>No results.</CommandEmpty>
        <CommandGroup heading="Fruit">
          <CommandItem value="Apple" onSelect={props.onSelect}>
            Apple
          </CommandItem>
          <CommandItem value="Banana" onSelect={props.onSelect}>
            Banana
          </CommandItem>
          <CommandItem value="Pineapple" disabled onSelect={props.onSelect}>
            Pineapple
          </CommandItem>
        </CommandGroup>
        <CommandSeparator />
        <CommandGroup heading="Tools">
          <CommandItem value="Hammer" keywords={[`nail`]} onSelect={props.onSelect}>
            Hammer
          </CommandItem>
          <CommandItem value="Apple pie tool" onSelect={props.onSelect}>
            Apple pie tool
          </CommandItem>
        </CommandGroup>
      </CommandList>
    </Command>
  )
}

const visible = (c: HTMLElement) => Array.from(c.querySelectorAll(`[data-slot="command-item"]`)).map((el) => el.getAttribute(`data-value`))
const selected = (c: HTMLElement) => c.querySelector(`[data-slot="command-item"][aria-selected="true"]`)?.getAttribute(`data-value`)
const input = (c: HTMLElement) => c.querySelector(`input`)!
const type = (c: HTMLElement, value: string) => fireEvent.change(input(c), { target: { value } })
const key = (c: HTMLElement, k: string, init: KeyboardEventInit = {}) => fireEvent.keyDown(input(c), { key: k, ...init })

describe(`Command`, () => {
  it(`shows every item in order and selects the first`, () => {
    const { container } = render(<Menu />)
    expect(visible(container)).toEqual([`Apple`, `Banana`, `Pineapple`, `Hammer`, `Apple pie tool`])
    expect(selected(container)).toBe(`Apple`)
    expect(input(container).getAttribute(`aria-activedescendant`)).toBe(container.querySelector(`[data-value="Apple"]`)!.id)
    expect(container.querySelector(`[data-slot="command-empty"]`)).toBeNull()
  })

  it(`filters and orders by score, groups by their best item, hides the separator`, () => {
    const { container } = render(<Menu />)
    type(container, `apple`)
    expect(visible(container)).toEqual([`Apple`, `Pineapple`, `Apple pie tool`])
    expect(selected(container)).toBe(`Apple`)
    expect(container.querySelector(`[data-slot="command-separator"]`)).toBeNull()
    type(container, `nail`)
    expect(visible(container)).toEqual([`Hammer`])
    const groups = container.querySelectorAll(`[data-slot="command-group"]`)
    expect(Array.from(groups).map((g) => [g.getAttribute(`data-value`), g.hasAttribute(`hidden`)])).toEqual([
      [`Tools`, false],
      [`Fruit`, true],
    ])
    expect(selected(container)).toBe(`Hammer`)
  })

  it(`restores the registration order when the query clears`, () => {
    const { container } = render(<Menu />)
    type(container, `tool`)
    type(container, ``)
    expect(visible(container)).toEqual([`Apple`, `Banana`, `Pineapple`, `Hammer`, `Apple pie tool`])
    expect(Array.from(container.querySelector(`[data-slot="command-list-sizer"]`)!.children).map((el) => el.getAttribute(`data-slot`))).toEqual([`command-group`, `command-separator`, `command-group`])
  })

  it(`shows the empty state when nothing matches`, () => {
    const { container } = render(<Menu />)
    type(container, `zzz`)
    expect(visible(container)).toEqual([])
    expect(container.querySelector(`[data-slot="command-empty"]`)!.textContent).toBe(`No results.`)
    expect(input(container).hasAttribute(`aria-activedescendant`)).toBe(false)
  })

  it(`starts from a default query`, () => {
    const { container } = render(<Menu query="ham" />)
    expect(visible(container)).toEqual([`Hammer`])
    expect(selected(container)).toBe(`Hammer`)
    expect(container.querySelector(`[data-slot="command-input-clear"]`)).not.toBeNull()
  })

  it(`walks with the arrows, skips disabled items, Home/End, loop, groups`, () => {
    const { container } = render(<Menu />)
    key(container, `ArrowDown`)
    expect(selected(container)).toBe(`Banana`)
    key(container, `ArrowDown`)
    expect(selected(container)).toBe(`Hammer`)
    key(container, `End`)
    expect(selected(container)).toBe(`Apple pie tool`)
    key(container, `ArrowDown`)
    expect(selected(container)).toBe(`Apple pie tool`)
    key(container, `Home`)
    expect(selected(container)).toBe(`Apple`)
    key(container, `ArrowUp`)
    expect(selected(container)).toBe(`Apple`)
    key(container, `ArrowDown`, { altKey: true })
    expect(selected(container)).toBe(`Hammer`)
    key(container, `ArrowUp`, { altKey: true })
    expect(selected(container)).toBe(`Apple`)
    key(container, `ArrowDown`, { metaKey: true })
    expect(selected(container)).toBe(`Apple pie tool`)
    key(container, `n`, { ctrlKey: true })
    expect(selected(container)).toBe(`Apple pie tool`)
    key(container, `p`, { ctrlKey: true })
    expect(selected(container)).toBe(`Hammer`)
  })

  it(`loops when asked`, () => {
    const { container } = render(<Menu loop />)
    key(container, `ArrowUp`)
    expect(selected(container)).toBe(`Apple pie tool`)
    key(container, `ArrowDown`)
    expect(selected(container)).toBe(`Apple`)
  })

  it(`Enter and a click fire onSelect; a disabled item never does`, () => {
    const onSelect = vi.fn()
    const { container } = render(<Menu onSelect={onSelect} />)
    key(container, `ArrowDown`)
    key(container, `Enter`)
    expect(onSelect).toHaveBeenLastCalledWith(`Banana`)
    fireEvent.click(container.querySelector(`[data-value="Hammer"]`)!)
    expect(onSelect).toHaveBeenLastCalledWith(`Hammer`)
    expect(selected(container)).toBe(`Hammer`)
    fireEvent.click(container.querySelector(`[data-value="Pineapple"]`)!)
    expect(onSelect).toHaveBeenCalledTimes(2)
  })

  it(`the clear button empties the query`, () => {
    const { container } = render(<Menu query="ham" />)
    fireEvent.click(container.querySelector(`[data-slot="command-input-clear"]`)!)
    expect(input(container).value).toBe(``)
    expect(visible(container)).toHaveLength(5)
    expect(container.querySelector(`[data-slot="command-input-clear"]`)).toBeNull()
  })

  it(`follows a controlled query and a controlled value`, () => {
    let setQuery: (q: string) => void = () => {}
    const onValueChange = vi.fn()
    function Controlled() {
      const [q, setQ] = useState(`ban`)
      setQuery = setQ
      return (
        <Command value="Banana" onValueChange={onValueChange}>
          <CommandInput value={q} onValueChange={setQ} />
          <CommandList>
            <CommandItem value="Apple">Apple</CommandItem>
            <CommandItem value="Banana">Banana</CommandItem>
          </CommandList>
        </Command>
      )
    }
    const { container } = render(<Controlled />)
    expect(visible(container)).toEqual([`Banana`])
    act(() => setQuery(``))
    expect(visible(container)).toEqual([`Apple`, `Banana`])
    expect(input(container).value).toBe(``)
    expect(selected(container)).toBe(`Banana`)
    key(container, `ArrowUp`)
    expect(onValueChange).toHaveBeenLastCalledWith(`Apple`)
    expect(selected(container)).toBe(`Banana`)
  })

  it(`defaults the value to the item's text`, () => {
    const { container } = render(
      <Command>
        <CommandList>
          <CommandItem>
            <span>Settings</span>
          </CommandItem>
        </CommandList>
      </Command>
    )
    expect(visible(container)).toEqual([`Settings`])
  })
})
