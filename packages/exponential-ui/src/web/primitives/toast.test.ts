// FEED-131: the toast engine against fixtures/toast-stack.json (every
// `geometry` case) and the store's clocks.

import { afterEach, beforeEach, describe, expect, it, jest } from "bun:test"
import fixture from "../../../fixtures/toast-stack.json" with { type: "json" }
import { TOAST_CONSTANTS, TOAST_TOUCH_QUERY, createToastStore, toastPlacement, toastStackGeometry } from "./toast"

describe(`toastStackGeometry`, () => {
  for (const c of fixture.geometry) {
    it(c.name, () => {
      expect(toastStackGeometry(c.heights, c.expanded, c.anchoredBottom)).toEqual({ height: c.height, items: c.items })
    })
  }
})

describe(`toastPlacement`, () => {
  it(`bottom-right on a pointer, top-centre on touch`, () => {
    expect(toastPlacement(false)).toEqual({ placement: `bottom-right`, anchoredBottom: true })
    expect(toastPlacement(true)).toEqual({ placement: `top-center`, anchoredBottom: false })
    expect(TOAST_TOUCH_QUERY).toBe(`(max-width: ${fixture.constants.touchMaxWidth}px)`)
    expect(TOAST_CONSTANTS).toEqual(fixture.constants)
  })
})

describe(`createToastStore`, () => {
  beforeEach(() => jest.useFakeTimers())
  afterEach(() => jest.useRealTimers())

  it(`queues toasts oldest first, kinds and ids`, () => {
    const store = createToastStore()
    const seen: string[][] = []
    store.subscribe((t) => seen.push(t.map((x) => x.id)))
    const a = store.toast(`Plain`)
    const b = store.toast.success(`Saved`, { description: `All good` })
    const c = store.toast({ id: `fixed`, title: `Options`, kind: `warning` })
    expect([a, b, c]).toEqual([`toast-1`, `toast-2`, `fixed`])
    expect(store.toasts().map((t) => [t.title, t.kind])).toEqual([[`Plain`, undefined], [`Saved`, `success`], [`Options`, `warning`]])
    expect(store.toasts()[1]!.description).toBe(`All good`)
    expect(seen.at(-1)).toEqual([`toast-1`, `toast-2`, `fixed`])
  })

  it(`auto-dismisses after durationMs; Infinity is sticky`, () => {
    const store = createToastStore()
    const dismissed: string[] = []
    store.toast(`Auto`, { onDismiss: (t) => dismissed.push(t.id) })
    store.toast(`Sticky`, { id: `s`, duration: Infinity })
    jest.advanceTimersByTime(TOAST_CONSTANTS.durationMs - 1)
    expect(store.toasts()).toHaveLength(2)
    jest.advanceTimersByTime(1)
    expect(store.toasts().map((t) => t.id)).toEqual([`s`])
    expect(dismissed).toEqual([`toast-1`])
  })

  it(`pauseAll keeps what is left of each clock, resumeAll restarts it`, () => {
    const store = createToastStore()
    store.toast(`One`, { duration: 1000 })
    jest.advanceTimersByTime(600)
    store.pauseAll()
    expect(store.paused).toBe(true)
    jest.advanceTimersByTime(5000)
    expect(store.toasts()).toHaveLength(1)
    // A toast added while paused waits too.
    store.toast(`Two`, { duration: 100 })
    jest.advanceTimersByTime(5000)
    expect(store.toasts()).toHaveLength(2)
    store.resumeAll()
    jest.advanceTimersByTime(100)
    expect(store.toasts().map((t) => t.title)).toEqual([`One`])
    jest.advanceTimersByTime(300)
    expect(store.toasts()).toHaveLength(0)
  })

  it(`dismiss(id) removes one, dismiss() all`, () => {
    const store = createToastStore()
    store.toast(`A`, { id: `a` })
    store.toast(`B`, { id: `b` })
    store.toast(`C`, { id: `c` })
    store.dismiss(`b`)
    expect(store.toasts().map((t) => t.id)).toEqual([`a`, `c`])
    store.dismiss()
    expect(store.toasts()).toEqual([])
  })

  it(`re-using a live id replaces the toast in place`, () => {
    const store = createToastStore()
    store.toast(`A`, { id: `a` })
    store.toast(`B`, { id: `b` })
    store.toast(`A2`, { id: `a` })
    expect(store.toasts().map((t) => t.title)).toEqual([`A2`, `B`])
  })
})

describe(`stackGeometry (N-12: the Dock pile draws the toast's stack)`, () => {
  it(`the toast's stack is stackGeometry with the toast's numbers`, async () => {
    const { stackGeometry, TOAST_CONSTANTS: C } = await import(`./toast`)
    for (const c of fixture.geometry) {
      expect(stackGeometry(c.heights, c.expanded, c.anchoredBottom, C)).toEqual({ height: c.height, items: c.items })
    }
  })
  it(`the Dock pile shares the toast's peek, scale step and visible count`, async () => {
    const { DOCK_PILE_CONSTANTS, dockPileGeometry } = await import(`./dock`)
    expect(DOCK_PILE_CONSTANTS.peek).toBe(fixture.constants.peek)
    expect(DOCK_PILE_CONSTANTS.scaleStep).toBe(fixture.constants.scaleStep)
    expect(DOCK_PILE_CONSTANTS.visible).toBe(fixture.constants.visible)
    expect(dockPileGeometry([60, 60, 60, 60], false)).toEqual({
      height: 102,
      items: [
        { offset: 0, scale: 0.9, visible: false },
        { offset: 14, scale: 0.9, visible: true },
        { offset: 28, scale: 0.95, visible: true },
        { offset: 42, scale: 1, visible: true },
      ],
    })
    expect(dockPileGeometry([60, 60], true)).toEqual({ height: 126, items: [{ offset: 0, scale: 1, visible: true }, { offset: 66, scale: 1, visible: true }] })
  })
})
