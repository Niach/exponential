// FEED-131: the Solid Select twin's behaviour (Radix Select's keyboard and
// value model) on @solidjs/testing-library.

import { createSignal } from "solid-js"
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./select"

const wait = (ms = 20) => new Promise((r) => setTimeout(r, ms))

function Fruits(props: { onValueChange?: (v: string) => void; defaultValue?: string }) {
  return (
    <Select onValueChange={props.onValueChange} defaultValue={props.defaultValue}>
      <SelectTrigger>
        <SelectValue placeholder="Pick a fruit" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="apple">Apple</SelectItem>
        <SelectItem value="banana">Banana</SelectItem>
        <SelectItem value="blueberry">Blueberry</SelectItem>
        <SelectItem value="cherry" disabled>
          Cherry
        </SelectItem>
      </SelectContent>
    </Select>
  )
}

afterEach(cleanup)

describe(`Select (Solid)`, () => {
  it(`shows the placeholder until a value is picked`, () => {
    render(() => <Fruits />)
    const trigger = screen.getByRole(`combobox`)
    expect(trigger.textContent).toBe(`Pick a fruit`)
    expect(trigger.hasAttribute(`data-placeholder`)).toBe(true)
    expect(trigger.getAttribute(`aria-expanded`)).toBe(`false`)
  })

  it(`opens from the keyboard, moves with the arrows and picks with Enter`, async () => {
    const picked: string[] = []
    render(() => <Fruits onValueChange={(v) => picked.push(v)} />)
    const trigger = screen.getByRole(`combobox`)
    trigger.focus()
    fireEvent.keyDown(trigger, { key: `ArrowDown` })
    await wait()
    expect(trigger.getAttribute(`aria-expanded`)).toBe(`true`)
    expect(screen.getByRole(`listbox`)).toBeTruthy()
    expect(document.activeElement?.textContent).toBe(`Apple`)
    fireEvent.keyDown(document.activeElement!, { key: `ArrowDown` })
    await wait()
    expect(document.activeElement?.textContent).toBe(`Banana`)
    expect(document.activeElement?.hasAttribute(`data-highlighted`)).toBe(true)
    fireEvent.keyDown(document.activeElement!, { key: `Enter` })
    await wait()
    expect(picked).toEqual([`banana`])
    expect(screen.queryByRole(`listbox`)).toBeNull()
    expect(trigger.textContent).toBe(`Banana`)
    expect(trigger.hasAttribute(`data-placeholder`)).toBe(false)
  })

  it(`skips a disabled item and focuses the selected one on open`, async () => {
    render(() => <Fruits defaultValue="blueberry" />)
    const trigger = screen.getByRole(`combobox`)
    fireEvent.keyDown(trigger, { key: `Enter` })
    await wait()
    expect(document.activeElement?.textContent).toBe(`Blueberry`)
    expect(document.activeElement?.getAttribute(`aria-selected`)).toBe(`true`)
    fireEvent.keyDown(document.activeElement!, { key: `ArrowDown` })
    await wait()
    expect(document.activeElement?.textContent).toBe(`Blueberry`)
  })

  it(`type-ahead on the closed trigger changes the value`, async () => {
    const picked: string[] = []
    render(() => <Fruits onValueChange={(v) => picked.push(v)} />)
    const trigger = screen.getByRole(`combobox`)
    fireEvent.keyDown(trigger, { key: `b` })
    expect(picked).toEqual([`banana`])
    fireEvent.keyDown(trigger, { key: `b` })
    expect(picked).toEqual([`banana`, `blueberry`])
    expect(trigger.textContent).toBe(`Blueberry`)
    expect(screen.queryByRole(`listbox`)).toBeNull()
  })

  it(`Escape closes and gives the focus back to the trigger`, async () => {
    render(() => <Fruits />)
    const trigger = screen.getByRole(`combobox`)
    fireEvent.keyDown(trigger, { key: ` ` })
    await wait()
    expect(screen.getByRole(`listbox`)).toBeTruthy()
    fireEvent.keyDown(document.activeElement!, { key: `Escape` })
    await wait()
    expect(screen.queryByRole(`listbox`)).toBeNull()
    expect(document.activeElement).toBe(trigger)
  })

  it(`is controlled by value / onValueChange`, async () => {
    const [value, setValue] = createSignal(`apple`)
    render(() => (
      <Select value={value()} onValueChange={setValue}>
        <SelectTrigger>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="apple">Apple</SelectItem>
          <SelectItem value="banana">Banana</SelectItem>
        </SelectContent>
      </Select>
    ))
    const trigger = screen.getByRole(`combobox`)
    expect(trigger.textContent).toBe(`Apple`)
    setValue(`banana`)
    expect(trigger.textContent).toBe(`Banana`)
  })
})
