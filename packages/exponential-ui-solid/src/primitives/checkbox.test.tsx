// FEED-131: the Solid Checkbox's Radix behaviour (state, indeterminate,
// controlled, the form bubble input).
import { describe, expect, it, vi } from "vitest"
import { createSignal } from "solid-js"
import { fireEvent, render, screen } from "@solidjs/testing-library"
import { Checkbox } from "./checkbox"

describe(`Checkbox`, () => {
  it(`toggles uncontrolled and reports the change`, () => {
    const onCheckedChange = vi.fn()
    render(() => <Checkbox aria-label="Accept" onCheckedChange={onCheckedChange} />)
    const box = screen.getByRole(`checkbox`)
    expect(box.getAttribute(`aria-checked`)).toBe(`false`)
    expect(box.querySelector(`[data-slot="checkbox-indicator"]`)).toBeNull()
    fireEvent.click(box)
    expect(box.getAttribute(`aria-checked`)).toBe(`true`)
    expect(box.getAttribute(`data-state`)).toBe(`checked`)
    expect(box.querySelector(`[data-slot="checkbox-indicator"]`)).toBeTruthy()
    expect(onCheckedChange).toHaveBeenCalledWith(true)
  })

  it(`indeterminate reads "mixed" and a click checks it`, () => {
    const [checked, setChecked] = createSignal<boolean | `indeterminate`>(`indeterminate`)
    render(() => <Checkbox aria-label="All" checked={checked()} onCheckedChange={setChecked} />)
    const box = screen.getByRole(`checkbox`)
    expect(box.getAttribute(`aria-checked`)).toBe(`mixed`)
    expect(box.getAttribute(`data-state`)).toBe(`indeterminate`)
    fireEvent.click(box)
    expect(checked()).toBe(true)
    expect(box.getAttribute(`aria-checked`)).toBe(`true`)
  })

  it(`stays put when controlled and the owner ignores the change`, () => {
    render(() => <Checkbox aria-label="Fixed" checked={false} />)
    const box = screen.getByRole(`checkbox`)
    fireEvent.click(box)
    expect(box.getAttribute(`aria-checked`)).toBe(`false`)
  })

  it(`Enter never toggles; disabled carries Radix's attributes`, () => {
    render(() => (
      <>
        <Checkbox aria-label="On" />
        <Checkbox aria-label="Off" disabled />
      </>
    ))
    const [on, off] = screen.getAllByRole(`checkbox`) as HTMLButtonElement[]
    const e = new KeyboardEvent(`keydown`, { key: `Enter`, bubbles: true, cancelable: true })
    on!.dispatchEvent(e)
    expect(e.defaultPrevented).toBe(true)
    expect(on!.getAttribute(`aria-checked`)).toBe(`false`)
    expect(off!.disabled).toBe(true)
    expect(off!.hasAttribute(`data-disabled`)).toBe(true)
  })

  it(`inside a <form> renders the bubble input, which follows the state`, () => {
    const onChange = vi.fn()
    const { container } = render(() => (
      <form onClick={onChange}>
        <Checkbox aria-label="Terms" name="terms" />
      </form>
    ))
    const input = container.querySelector(`input[type="checkbox"]`) as HTMLInputElement
    expect(input).toBeTruthy()
    expect(input.name).toBe(`terms`)
    expect(input.value).toBe(`on`)
    fireEvent.click(screen.getByRole(`checkbox`))
    expect(input.checked).toBe(true)
    expect(new FormData(container.querySelector(`form`)!).get(`terms`)).toBe(`on`)
  })
})
