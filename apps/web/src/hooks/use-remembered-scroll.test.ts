import { renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { useRememberedScroll } from "@/hooks/use-remembered-scroll"
import {
  readTabMemory,
  resetTabMemory,
  writeTabMemory,
} from "@/lib/work-tab-memory"

// EXP-894 regression: the restore window must close even where nothing
// observes resizes (jsdom, an old WebView, a browser without
// `ResizeObserver`). While `restoring` is true the hook records NOTHING, so a
// window that never closes silently turns the memory read-only for the life
// of the mount.

type Listener = () => void

/** A scroll container with a real clamp: `scrollTop` can never exceed the
 *  content's overflow, which is exactly the case the stashed offset can be
 *  unreachable in. */
function fakeScroller(initialMaxScroll: number) {
  const listeners = new Map<string, Set<Listener>>()
  let maxScroll = initialMaxScroll
  let top = 0
  const node = {
    children: [] as Element[],
    get scrollTop() {
      return top
    },
    set scrollTop(value: number) {
      top = Math.max(0, Math.min(maxScroll, value))
    },
    addEventListener(type: string, fn: Listener) {
      const set = listeners.get(type) ?? new Set<Listener>()
      set.add(fn)
      listeners.set(type, set)
    },
    removeEventListener(type: string, fn: Listener) {
      listeners.get(type)?.delete(fn)
    },
  }
  return {
    node: node as unknown as HTMLElement,
    /** The reader (or the browser's anchoring) scrolls. */
    scrollTo(value: number) {
      top = Math.max(0, Math.min(maxScroll, value))
      for (const fn of listeners.get(`scroll`) ?? []) fn()
    },
    /** Content lands after mount, so the overflow grows. */
    grow(next: number) {
      maxScroll = next
    },
    get top() {
      return top
    },
  }
}

describe(`useRememberedScroll`, () => {
  const originalResizeObserver = globalThis.ResizeObserver

  beforeEach(() => {
    resetTabMemory()
    vi.useFakeTimers()
    // The environment under test: no resize observation at all.
    // @ts-expect-error — deleting an optional global for the test.
    delete globalThis.ResizeObserver
  })
  afterEach(() => {
    vi.useRealTimers()
    globalThis.ResizeObserver = originalResizeObserver
    resetTabMemory()
  })

  it(`restores the remembered offset on mount`, () => {
    writeTabMemory(`issue:1`, `scroll`, 120)
    const scroller = fakeScroller(1000)
    const { result } = renderHook(() => useRememberedScroll(`issue:1`, `scroll`))
    result.current(scroller.node)
    expect(scroller.top).toBe(120)
  })

  it(`keeps recording after an UNREACHABLE offset with no ResizeObserver`, () => {
    // The stash points past what the remounted content can scroll to.
    writeTabMemory(`issue:1`, `scroll`, 900)
    const scroller = fakeScroller(100)
    const { result } = renderHook(() => useRememberedScroll(`issue:1`, `scroll`))
    result.current(scroller.node)
    expect(scroller.top).toBe(100)

    // Inside the window the restore still owns the scroller: a scroll event is
    // the restore's own echo, not the reader.
    scroller.scrollTo(30)
    expect(readTabMemory(`issue:1`, `scroll`)).toBe(900)

    // Once the window closes the hook records again — before the fix
    // `restoring` stayed true forever, because the timer was armed only
    // alongside a ResizeObserver.
    vi.advanceTimersByTime(1_500)
    scroller.scrollTo(42)
    expect(readTabMemory(`issue:1`, `scroll`)).toBe(42)
  })

  it(`stops restoring the moment the offset is reached`, () => {
    writeTabMemory(`issue:1`, `scroll`, 120)
    const scroller = fakeScroller(1000)
    const { result } = renderHook(() => useRememberedScroll(`issue:1`, `scroll`))
    result.current(scroller.node)
    // The restore landed on mount, so the next scroll is the reader's — no
    // waiting out the window.
    scroller.scrollTo(300)
    expect(readTabMemory(`issue:1`, `scroll`)).toBe(300)
  })

  it(`treats a landing scroll event as the end of the restore`, () => {
    writeTabMemory(`issue:1`, `scroll`, 500)
    // Content that grows AFTER mount: the clamp keeps the first apply short,
    // then the browser's own anchoring lands on the target.
    const scroller = fakeScroller(200)
    const { result } = renderHook(() => useRememberedScroll(`issue:1`, `scroll`))
    result.current(scroller.node)
    expect(scroller.top).toBe(200)
    scroller.grow(1000)
    // The browser anchors the grown content back onto the target: that scroll
    // is the restore landing, so the reader owns the scroller from here.
    scroller.scrollTo(500)
    scroller.scrollTo(560)
    expect(readTabMemory(`issue:1`, `scroll`)).toBe(560)
  })

  it(`clears the slot when the reader scrolls back to the top`, () => {
    const scroller = fakeScroller(1000)
    const { result } = renderHook(() => useRememberedScroll(`issue:2`, `scroll`))
    result.current(scroller.node)
    scroller.scrollTo(80)
    expect(readTabMemory(`issue:2`, `scroll`)).toBe(80)
    scroller.scrollTo(0)
    expect(readTabMemory(`issue:2`, `scroll`)).toBeUndefined()
  })
})
