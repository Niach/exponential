import { act, renderHook, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"
import { useKeyboardInset } from "@/hooks/use-keyboard-inset"

// EXP-568 — the phone formatting bar rides this value, so a missed listener
// leaves the rail parked under the keyboard (resize) or drifting off the
// keyboard's edge while iOS pans the visual viewport (scroll).

type Listener = () => void

function stubViewport(layoutHeight: number, offsetTop: number, height: number) {
  const listeners = new Map<string, Set<Listener>>()
  const viewport = {
    offsetTop,
    height,
    addEventListener: (type: string, fn: Listener) => {
      const set = listeners.get(type) ?? new Set()
      set.add(fn)
      listeners.set(type, set)
    },
    removeEventListener: (type: string, fn: Listener) => {
      listeners.get(type)?.delete(fn)
    },
  }
  Object.defineProperty(window, `innerHeight`, {
    configurable: true,
    value: layoutHeight,
  })
  Object.defineProperty(window, `visualViewport`, {
    configurable: true,
    value: viewport,
  })
  return {
    viewport,
    fire: (type: string) => {
      for (const fn of listeners.get(type) ?? []) fn()
    },
    listenerCount: (type: string) => listeners.get(type)?.size ?? 0,
  }
}

const flushFrames = async () => {
  await act(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => resolve(null)))
    await new Promise((resolve) => requestAnimationFrame(() => resolve(null)))
  })
}

afterEach(() => {
  Object.defineProperty(window, `visualViewport`, {
    configurable: true,
    value: null,
  })
})

describe(`useKeyboardInset`, () => {
  it(`is zero and closed without a visualViewport (older browsers)`, () => {
    Object.defineProperty(window, `visualViewport`, {
      configurable: true,
      value: null,
    })
    const { result } = renderHook(() => useKeyboardInset())
    expect(result.current).toEqual({ inset: 0, keyboardOpen: false })
  })

  it(`measures the occluded band on mount`, async () => {
    stubViewport(800, 0, 480)
    const { result } = renderHook(() => useKeyboardInset())
    await flushFrames()
    await waitFor(() => expect(result.current.inset).toBe(320))
    expect(result.current.keyboardOpen).toBe(true)
  })

  it(`re-measures on resize when the keyboard closes`, async () => {
    const stub = stubViewport(800, 0, 480)
    const { result } = renderHook(() => useKeyboardInset())
    await flushFrames()
    await waitFor(() => expect(result.current.inset).toBe(320))

    stub.viewport.height = 800
    stub.fire(`resize`)
    await flushFrames()
    await waitFor(() => expect(result.current.inset).toBe(0))
    expect(result.current.keyboardOpen).toBe(false)
  })

  it(`re-measures on scroll (iOS visual-viewport panning)`, async () => {
    const stub = stubViewport(800, 0, 480)
    const { result } = renderHook(() => useKeyboardInset())
    await flushFrames()
    await waitFor(() => expect(result.current.inset).toBe(320))

    stub.viewport.offsetTop = 120
    stub.fire(`scroll`)
    await flushFrames()
    await waitFor(() => expect(result.current.inset).toBe(200))
  })

  it(`stays closed for a URL-bar-sized change`, async () => {
    stubViewport(800, 0, 760)
    const { result } = renderHook(() => useKeyboardInset())
    await flushFrames()
    await waitFor(() => expect(result.current.inset).toBe(40))
    expect(result.current.keyboardOpen).toBe(false)
  })

  it(`unsubscribes both listeners on unmount`, async () => {
    const stub = stubViewport(800, 0, 480)
    const { unmount } = renderHook(() => useKeyboardInset())
    await flushFrames()
    expect(stub.listenerCount(`resize`)).toBe(1)
    expect(stub.listenerCount(`scroll`)).toBe(1)
    unmount()
    expect(stub.listenerCount(`resize`)).toBe(0)
    expect(stub.listenerCount(`scroll`)).toBe(0)
  })
})
