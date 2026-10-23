// FEED-131: THE toast on Solid (React's toast.test.tsx): the stack placed by fixtures/toast-stack.json
// (every geometry case through a live <Toaster>), the clocks on fake time,
// hover pauses them, the close glyph and the action dismiss.

import { fireEvent, render } from "@solidjs/testing-library"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import fixture from "@exponential_app/ui/fixtures/toast-stack.json"
import { TOAST_CLASSES } from "@exponential_app/ui/web/primitives"
import { Toaster, createToastStore, toast, toastStore } from "./toast"

const cards = () => [...document.querySelectorAll<HTMLElement>(`[data-slot="toast"]`)]
const list = () => document.querySelector<HTMLElement>(`[data-slot="toast-list"]`)!

describe(`Toaster geometry (fixtures/toast-stack.json)`, () => {
  for (const c of fixture.geometry) {
    it(c.name, () => {
      const store = createToastStore()
      c.heights.forEach((height, i) => store.toast(`Toast ${i}`, { id: `t${i}`, duration: Infinity, height }))
      render(() => <Toaster store={store} touch={!c.anchoredBottom} />)
      if (c.expanded) fireEvent.pointerEnter(list(), { pointerType: `mouse` })
      expect(list().style.height).toBe(`${c.height}px`)
      expect(cards().map((el) => ({ offset: parseFloat(el.style.getPropertyValue(`--offset`)), scale: Number(el.style.getPropertyValue(`--scale`)), visible: el.dataset.visible === `true` }))).toEqual(c.items)
      expect(list().dataset.expanded).toBe(c.heights.length > 0 && c.expanded ? `true` : `false`)
    })
  }
})

describe(`Toaster`, () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => {
    toastStore.dismiss()
    vi.useRealTimers()
  })

  it(`the default store: a card with its kind glyph, title, description, action, close`, () => {
    render(() => <Toaster touch={false} />)
    toast.success(`Saved`, { description: `All good`, action: { label: `Undo` } })
    const card = cards()[0]!
    expect(card.getAttribute(`role`)).toBe(`status`)
    expect(card.dataset.kind).toBe(`success`)
    expect(card.dataset.front).toBe(`true`)
    expect(card.querySelector(`[data-toast-kind="success"]`)!.getAttribute(`class`)).toContain(TOAST_CLASSES.kindIcon.success)
    expect(card.querySelector(`[data-slot="toast-title"]`)!.textContent).toBe(`Saved`)
    expect(card.querySelector(`[data-slot="toast-description"]`)!.textContent).toBe(`All good`)
    expect(card.querySelector(`[data-slot="toast-action"]`)!.textContent).toBe(`Undo`)
    expect(card.querySelector(`[data-slot="toast-close"]`)!.getAttribute(`aria-label`)).toBe(`Close`)
    expect(document.querySelector(`[data-slot="toaster"]`)!.getAttribute(`data-placement`)).toBe(`bottom-right`)
  })

  it(`auto-dismisses after durationMs; hover pauses the clock`, () => {
    render(() => <Toaster touch={false} />)
    toast(`Auto`)
    vi.advanceTimersByTime(fixture.constants.durationMs - 1)
    expect(cards()).toHaveLength(1)
    vi.advanceTimersByTime(1)
    expect(cards()).toHaveLength(0)

    toast(`Paused`, { duration: 1000 })
    vi.advanceTimersByTime(500)
    fireEvent.pointerEnter(list(), { pointerType: `mouse` })
    vi.advanceTimersByTime(5000)
    expect(cards()).toHaveLength(1)
    expect(list().dataset.expanded).toBe(`true`)
    fireEvent.pointerLeave(list(), { pointerType: `mouse` })
    vi.advanceTimersByTime(499)
    expect(cards()).toHaveLength(1)
    vi.advanceTimersByTime(1)
    expect(cards()).toHaveLength(0)
  })

  it(`the close glyph and the action dismiss; a tap expands on touch`, () => {
    const onClick = vi.fn()
    render(() => <Toaster touch />)
    toast.info(`One`, { id: `one` })
    toast.error(`Two`, { id: `two`, action: { label: `Retry`, onClick } })
    expect(document.querySelector(`[data-slot="toaster"]`)!.getAttribute(`data-placement`)).toBe(`top-center`)
    fireEvent.pointerDown(list(), { pointerType: `touch` })
    expect(list().dataset.expanded).toBe(`true`)
    fireEvent.click(cards()[1]!.querySelector(`[data-slot="toast-action"]`)!)
    expect(onClick).toHaveBeenCalledTimes(1)
    expect(cards().map((el) => el.textContent)).toEqual([`One`])
    fireEvent.click(cards()[0]!.querySelector(`[data-slot="toast-close"]`)!)
    expect(cards()).toHaveLength(0)
  })
})
