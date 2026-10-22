// FEED-131: the Solid twin of exponential-ui-react's command.test.tsx: the
// same cases (the post-render pass runs in a microtask here: `settle`).

import { describe, expect, it, vi } from "vitest"
import { fireEvent, render } from "@solidjs/testing-library"
import { createSignal } from "solid-js"
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
const settle = async () => {
  for (let i = 0; i < 5; i++) await Promise.resolve()
}
const type = async (c: HTMLElement, value: string) => {
  fireEvent.input(input(c), { target: { value } })
  await settle()
}
const key = async (c: HTMLElement, k: string, init: KeyboardEventInit = {}) => {
  fireEvent.keyDown(input(c), { key: k, ...init })
  await settle()
}
const mount = async (ui: () => unknown) => {
  const r = render(ui as never)
  await settle()
  return r
}

describe(`Command`, () => {
  it(`shows every item in order and selects the first`, async () => {
    const { container } = await mount(() => <Menu />)
    expect(visible(container)).toEqual([`Apple`, `Banana`, `Pineapple`, `Hammer`, `Apple pie tool`])
    expect(selected(container)).toBe(`Apple`)
    expect(input(container).getAttribute(`aria-activedescendant`)).toBe(container.querySelector(`[data-value="Apple"]`)!.id)
    expect(container.querySelector(`[data-slot="command-empty"]`)).toBeNull()
  })

  it(`filters and orders by score, groups by their best item, hides the separator`, async () => {
    const { container } = await mount(() => <Menu />)
    await type(container, `apple`)
    expect(visible(container)).toEqual([`Apple`, `Pineapple`, `Apple pie tool`])
    expect(selected(container)).toBe(`Apple`)
    expect(container.querySelector(`[data-slot="command-separator"]`)).toBeNull()
    await type(container, `nail`)
    expect(visible(container)).toEqual([`Hammer`])
    const groups = container.querySelectorAll(`[data-slot="command-group"]`)
    expect(Array.from(groups).map((g) => [g.getAttribute(`data-value`), g.hasAttribute(`hidden`)])).toEqual([
      [`Tools`, false],
      [`Fruit`, true],
    ])
    expect(selected(container)).toBe(`Hammer`)
  })

  it(`restores the registration order when the query clears`, async () => {
    const { container } = await mount(() => <Menu />)
    await type(container, `tool`)
    await type(container, ``)
    expect(visible(container)).toEqual([`Apple`, `Banana`, `Pineapple`, `Hammer`, `Apple pie tool`])
    expect(Array.from(container.querySelector(`[data-slot="command-list-sizer"]`)!.children).map((el) => el.getAttribute(`data-slot`))).toEqual([`command-group`, `command-separator`, `command-group`])
  })

  it(`shows the empty state when nothing matches`, async () => {
    const { container } = await mount(() => <Menu />)
    await type(container, `zzz`)
    expect(visible(container)).toEqual([])
    expect(container.querySelector(`[data-slot="command-empty"]`)!.textContent).toBe(`No results.`)
    expect(input(container).hasAttribute(`aria-activedescendant`)).toBe(false)
  })

  it(`starts from a default query`, async () => {
    const { container } = await mount(() => <Menu query="ham" />)
    expect(visible(container)).toEqual([`Hammer`])
    expect(selected(container)).toBe(`Hammer`)
    expect(container.querySelector(`[data-slot="command-input-clear"]`)).not.toBeNull()
  })

  it(`walks with the arrows, skips disabled items, Home/End, loop, groups`, async () => {
    const { container } = await mount(() => <Menu />)
    await key(container, `ArrowDown`)
    expect(selected(container)).toBe(`Banana`)
    await key(container, `ArrowDown`)
    expect(selected(container)).toBe(`Hammer`)
    await key(container, `End`)
    expect(selected(container)).toBe(`Apple pie tool`)
    await key(container, `ArrowDown`)
    expect(selected(container)).toBe(`Apple pie tool`)
    await key(container, `Home`)
    expect(selected(container)).toBe(`Apple`)
    await key(container, `ArrowUp`)
    expect(selected(container)).toBe(`Apple`)
    await key(container, `ArrowDown`, { altKey: true })
    expect(selected(container)).toBe(`Hammer`)
    await key(container, `ArrowUp`, { altKey: true })
    expect(selected(container)).toBe(`Apple`)
    await key(container, `ArrowDown`, { metaKey: true })
    expect(selected(container)).toBe(`Apple pie tool`)
    await key(container, `n`, { ctrlKey: true })
    expect(selected(container)).toBe(`Apple pie tool`)
    await key(container, `p`, { ctrlKey: true })
    expect(selected(container)).toBe(`Hammer`)
  })

  it(`loops when asked`, async () => {
    const { container } = await mount(() => <Menu loop />)
    await key(container, `ArrowUp`)
    expect(selected(container)).toBe(`Apple pie tool`)
    await key(container, `ArrowDown`)
    expect(selected(container)).toBe(`Apple`)
  })

  it(`Enter and a click fire onSelect; a disabled item never does`, async () => {
    const onSelect = vi.fn()
    const { container } = await mount(() => <Menu onSelect={onSelect} />)
    await key(container, `ArrowDown`)
    await key(container, `Enter`)
    expect(onSelect).toHaveBeenLastCalledWith(`Banana`)
    fireEvent.click(container.querySelector(`[data-value="Hammer"]`)!)
    await settle()
    expect(onSelect).toHaveBeenLastCalledWith(`Hammer`)
    expect(selected(container)).toBe(`Hammer`)
    fireEvent.click(container.querySelector(`[data-value="Pineapple"]`)!)
    expect(onSelect).toHaveBeenCalledTimes(2)
  })

  it(`the clear button empties the query`, async () => {
    const { container } = await mount(() => <Menu query="ham" />)
    fireEvent.click(container.querySelector(`[data-slot="command-input-clear"]`)!)
    await settle()
    expect(input(container).value).toBe(``)
    expect(visible(container)).toHaveLength(5)
    expect(container.querySelector(`[data-slot="command-input-clear"]`)).toBeNull()
  })

  it(`follows a controlled query and a controlled value`, async () => {
    let setQuery: (q: string) => void = () => {}
    const onValueChange = vi.fn()
    function Controlled() {
      const [q, setQ] = createSignal(`ban`)
      setQuery = setQ
      return (
        <Command value="Banana" onValueChange={onValueChange}>
          <CommandInput value={q()} onValueChange={setQ} />
          <CommandList>
            <CommandItem value="Apple">Apple</CommandItem>
            <CommandItem value="Banana">Banana</CommandItem>
          </CommandList>
        </Command>
      )
    }
    const { container } = await mount(() => <Controlled />)
    expect(visible(container)).toEqual([`Banana`])
    setQuery(``)
    await settle()
    expect(visible(container)).toEqual([`Apple`, `Banana`])
    expect(input(container).value).toBe(``)
    expect(selected(container)).toBe(`Banana`)
    await key(container, `ArrowUp`)
    expect(onValueChange).toHaveBeenLastCalledWith(`Apple`)
    expect(selected(container)).toBe(`Banana`)
  })

  it(`defaults the value to the item's text`, async () => {
    const { container } = await mount(() => 
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
