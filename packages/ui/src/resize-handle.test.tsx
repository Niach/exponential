import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { ResizeHandle } from "./resize-handle"

// EXP-1156: the column edge — a separator that drags, steps and resets.

function setup(props: Partial<Parameters<typeof ResizeHandle>[0]> = {}) {
  const onChange = vi.fn()
  const onCommit = vi.fn()
  const onReset = vi.fn()
  const onDraggingChange = vi.fn()
  render(
    <ResizeHandle
      aria-label="Resize sidebar"
      value={300}
      min={272}
      max={560}
      step={16}
      onChange={onChange}
      onCommit={onCommit}
      onReset={onReset}
      onDraggingChange={onDraggingChange}
      {...props}
    />
  )
  const handle = screen.getByRole(`separator`, { name: `Resize sidebar` })
  // jsdom has no pointer capture.
  handle.setPointerCapture = vi.fn()
  return { handle, onChange, onCommit, onReset, onDraggingChange }
}

describe(`ResizeHandle`, () => {
  it(`is a focusable vertical separator carrying its value`, () => {
    const { handle } = setup()
    expect(handle.getAttribute(`aria-orientation`)).toBe(`vertical`)
    expect(handle.getAttribute(`aria-valuenow`)).toBe(`300`)
    expect(handle.getAttribute(`aria-valuemin`)).toBe(`272`)
    expect(handle.getAttribute(`aria-valuemax`)).toBe(`560`)
    expect(handle.tabIndex).toBe(0)
    expect(handle.className).toContain(`cursor-col-resize`)
  })

  it(`drags from the start width, clamped, and commits on release`, () => {
    const { handle, onChange, onCommit, onDraggingChange } = setup()
    fireEvent.pointerDown(handle, { pointerId: 1, button: 0, clientX: 100 })
    expect(onDraggingChange).toHaveBeenLastCalledWith(true)
    expect(document.body.style.cursor).toBe(`col-resize`)
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 150 })
    expect(onChange).toHaveBeenLastCalledWith(350)
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 900 })
    expect(onChange).toHaveBeenLastCalledWith(560)
    fireEvent.pointerUp(handle, { pointerId: 1, clientX: 900 })
    expect(onCommit).toHaveBeenCalledWith(560)
    expect(onDraggingChange).toHaveBeenLastCalledWith(false)
    expect(document.body.style.cursor).toBe(``)
  })

  it(`converts pointer pixels through the scale`, () => {
    const { handle, onChange } = setup({ scale: 2 })
    fireEvent.pointerDown(handle, { pointerId: 1, button: 0, clientX: 0 })
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 40 })
    expect(onChange).toHaveBeenLastCalledWith(320)
  })

  it(`steps with the arrow keys, four times with Shift`, () => {
    const { handle, onChange, onCommit } = setup()
    fireEvent.keyDown(handle, { key: `ArrowRight` })
    expect(onChange).toHaveBeenLastCalledWith(316)
    expect(onCommit).toHaveBeenLastCalledWith(316)
    fireEvent.keyDown(handle, { key: `ArrowLeft`, shiftKey: true })
    expect(onChange).toHaveBeenLastCalledWith(272)
    fireEvent.keyDown(handle, { key: `End` })
    expect(onChange).toHaveBeenLastCalledWith(560)
  })

  it(`resets on double-click`, () => {
    const { handle, onReset } = setup()
    fireEvent.doubleClick(handle)
    expect(onReset).toHaveBeenCalledTimes(1)
  })
})
