// FEED-131: the Solid Switch's Radix behaviour.
import { describe, expect, it, vi } from "vitest"
import { fireEvent, render, screen } from "@solidjs/testing-library"
import { Switch } from "./switch"

describe(`Switch`, () => {
  it(`toggles, moves the thumb state and reports the change`, () => {
    const onCheckedChange = vi.fn()
    render(() => <Switch aria-label="Wi-Fi" onCheckedChange={onCheckedChange} />)
    const sw = screen.getByRole(`switch`)
    const thumb = sw.querySelector(`[data-slot="switch-thumb"]`)!
    expect(sw.getAttribute(`data-size`)).toBe(`default`)
    expect(thumb.getAttribute(`data-state`)).toBe(`unchecked`)
    fireEvent.click(sw)
    expect(sw.getAttribute(`aria-checked`)).toBe(`true`)
    expect(thumb.getAttribute(`data-state`)).toBe(`checked`)
    expect(onCheckedChange).toHaveBeenCalledWith(true)
  })

  it(`size sm, disabled`, () => {
    render(() => <Switch aria-label="Dense" size="sm" disabled />)
    const sw = screen.getByRole(`switch`) as HTMLButtonElement
    expect(sw.getAttribute(`data-size`)).toBe(`sm`)
    expect(sw.disabled).toBe(true)
  })

  it(`inside a <form>: the bubble input carries the state as its checked attribute`, () => {
    const { container } = render(() => (
      <form>
        <Switch aria-label="Wi-Fi" name="wifi" defaultChecked />
      </form>
    ))
    const input = container.querySelector(`input`) as HTMLInputElement
    expect(input.hasAttribute(`checked`)).toBe(true)
    fireEvent.click(screen.getByRole(`switch`))
    expect(input.hasAttribute(`checked`)).toBe(false)
    expect(input.checked).toBe(false)
  })
})
