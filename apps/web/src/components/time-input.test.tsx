import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { TimeInput } from "@/components/time-input"

function renderInput(value: string | null = null) {
  const onChange = vi.fn()
  render(
    <TimeInput value={value} onChange={onChange} ariaLabel="Start time" />
  )
  const input = screen.getByLabelText(`Start time`) as HTMLInputElement
  return { input, onChange }
}

function commit(input: HTMLInputElement, raw: string) {
  fireEvent.change(input, { target: { value: raw } })
  fireEvent.blur(input)
}

describe(`TimeInput commit parsing`, () => {
  it.each([
    [`9:5`, `09:05`],
    [`9:`, `09:00`],
    [`9`, `09:00`],
    [`13`, `13:00`],
    [`13:37`, `13:37`],
    [`1337`, `13:37`],
    [`930`, `09:30`],
  ])(`normalizes %s to %s`, (raw, expected) => {
    const { input, onChange } = renderInput()
    commit(input, raw)
    expect(onChange).toHaveBeenCalledWith(expected)
    expect(input.value).toBe(expected)
  })

  it(`clears to null on an empty entry`, () => {
    const { input, onChange } = renderInput(`08:30`)
    commit(input, ``)
    expect(onChange).toHaveBeenCalledWith(null)
    expect(input.value).toBe(``)
  })

  it.each([`25:00`, `12:75`, `9:99`])(
    `reverts invalid entry %s to the prior value`,
    (raw) => {
      const { input, onChange } = renderInput(`08:30`)
      commit(input, raw)
      expect(onChange).not.toHaveBeenCalled()
      expect(input.value).toBe(`08:30`)
    }
  )
})
