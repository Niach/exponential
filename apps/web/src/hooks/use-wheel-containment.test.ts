import { describe, expect, it } from "vitest"
import { renderHook } from "@testing-library/react"
import {
  useWheelContainment,
  wheelHasSomewhereToGo,
} from "@/hooks/use-wheel-containment"

// EXP-619: the rule the agent dock leans on — a wheel tick escapes the
// terminal only while something inside it can still move.

function el(
  overflow: string,
  metrics: { scrollTop?: number; clientHeight?: number; scrollHeight?: number }
): HTMLDivElement {
  const node = document.createElement(`div`)
  node.style.overflowY = overflow
  node.style.overflowX = overflow
  // jsdom does no layout, so the geometry is stubbed.
  Object.defineProperty(node, `scrollTop`, {
    value: metrics.scrollTop ?? 0,
    writable: true,
  })
  Object.defineProperty(node, `clientHeight`, {
    value: metrics.clientHeight ?? 0,
  })
  Object.defineProperty(node, `scrollHeight`, {
    value: metrics.scrollHeight ?? 0,
  })
  return node
}

function tree(scroller: HTMLElement) {
  const root = document.createElement(`div`)
  const target = document.createElement(`span`)
  root.appendChild(scroller)
  scroller.appendChild(target)
  return { root, target }
}

describe(`wheelHasSomewhereToGo`, () => {
  it(`lets a scroller with room downwards through`, () => {
    const { root, target } = tree(
      el(`auto`, { scrollTop: 0, clientHeight: 100, scrollHeight: 400 })
    )
    expect(wheelHasSomewhereToGo(target, root, 0, 50)).toBe(true)
  })

  it(`contains a feed parked at its bottom edge`, () => {
    const { root, target } = tree(
      el(`auto`, { scrollTop: 300, clientHeight: 100, scrollHeight: 400 })
    )
    expect(wheelHasSomewhereToGo(target, root, 0, 50)).toBe(false)
    // …while scrolling back up is still the feed's own gesture.
    expect(wheelHasSomewhereToGo(target, root, 0, -50)).toBe(true)
  })

  it(`contains a short feed that cannot scroll at all`, () => {
    const { root, target } = tree(
      el(`auto`, { scrollTop: 0, clientHeight: 400, scrollHeight: 400 })
    )
    expect(wheelHasSomewhereToGo(target, root, 0, 50)).toBe(false)
  })

  it(`contains chrome with no scroller between it and the root`, () => {
    const { root, target } = tree(
      el(`hidden`, { scrollTop: 0, clientHeight: 100, scrollHeight: 400 })
    )
    expect(wheelHasSomewhereToGo(target, root, 0, 50)).toBe(false)
  })

  it(`stops the walk at the root instead of consulting the page`, () => {
    const page = el(`auto`, {
      scrollTop: 0,
      clientHeight: 100,
      scrollHeight: 4000,
    })
    const { root, target } = tree(el(`hidden`, {}))
    page.appendChild(root)
    expect(wheelHasSomewhereToGo(target, root, 0, 50)).toBe(false)
  })

  it(`ignores a vertical scroller for a horizontal gesture`, () => {
    const { root, target } = tree(
      el(`auto`, { scrollTop: 0, clientHeight: 100, scrollHeight: 400 })
    )
    expect(wheelHasSomewhereToGo(target, root, 50, 0)).toBe(false)
  })
})

describe(`useWheelContainment`, () => {
  function mount() {
    const { result, unmount } = renderHook(() =>
      useWheelContainment<HTMLDivElement>()
    )
    const root = document.createElement(`div`)
    document.body.appendChild(root)
    return { attach: result.current, root, unmount }
  }

  function wheel(target: Element, init: WheelEventInit) {
    const event = new WheelEvent(`wheel`, {
      bubbles: true,
      cancelable: true,
      ...init,
    })
    target.dispatchEvent(event)
    return event
  }

  it(`swallows a wheel tick nothing in the panel can use`, () => {
    const { attach, root } = mount()
    const chrome = el(`hidden`, {})
    root.appendChild(chrome)
    attach(root)
    expect(wheel(chrome, { deltaY: 50 }).defaultPrevented).toBe(true)
  })

  it(`leaves a scrollable feed alone`, () => {
    const { attach, root } = mount()
    const feed = el(`auto`, { scrollTop: 0, clientHeight: 100, scrollHeight: 400 })
    root.appendChild(feed)
    attach(root)
    expect(wheel(feed, { deltaY: 50 }).defaultPrevented).toBe(false)
  })

  it(`never touches ctrl+wheel — that is pinch-zoom`, () => {
    const { attach, root } = mount()
    attach(root)
    expect(wheel(root, { deltaY: 50, ctrlKey: true }).defaultPrevented).toBe(
      false
    )
  })

  it(`detaches when the panel unmounts`, () => {
    const { attach, root } = mount()
    attach(root)
    attach(null)
    expect(wheel(root, { deltaY: 50 }).defaultPrevented).toBe(false)
  })
})
