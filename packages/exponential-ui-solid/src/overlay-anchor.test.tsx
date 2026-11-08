// #EXP-1739: interactions-round1.json `overlayAnchor` on Solid, like React's
// interactions-fixture.test.tsx: an open anchored Popover closes once its
// anchor scrolls, moves or leaves the tree. jsdom lays nothing out, so a
// box's origin here is its place among its siblings (40px a row): a node
// inserted above the anchor moves it.

import { afterEach, describe, expect, it, vi } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import { createSignal } from "solid-js"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

interface OverlayAnchorCase {
  name: string
  step: { relayout?: boolean; scroll?: string; y?: number; insertAbove?: boolean; removeAnchor?: boolean }
  after: boolean
}

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root

describe(`fixtures/interactions-round1.json overlayAnchor`, () => {
  const fx = (fixture as unknown as { overlayAnchor: { surface: NestedNode; above: NestedNode; cases: OverlayAnchorCase[] } }).overlayAnchor
  for (const c of fx.cases) {
    it(`overlayAnchor: ${c.name}`, async () => {
      const rect = vi.spyOn(HTMLElement.prototype, `getBoundingClientRect`).mockImplementation(function (this: HTMLElement) {
        let y = 0
        for (let el: Element | null = this.closest(`[data-xui-id]`); el?.previousElementSibling; el = el.previousElementSibling) y += 40
        return new DOMRect(0, y, 100, 40)
      })
      const shown = () => document.querySelector(`[data-xui-overlay="Popover"]`) !== null
      const frames = () => new Promise((resolve) => setTimeout(resolve, 50))
      const [root, setRoot] = createSignal(tree(fx.surface))
      const { container } = render(() => <ExponentialSurface root={root()} theme="neutral" id="oa" />)
      await frames()
      expect(shown(), `${c.name}: opens against its anchor`).toBe(true)
      const children = [...(fx.surface.children ?? [])] as NestedNode[]
      if (c.step.insertAbove) children.unshift(fx.above)
      const next = c.step.removeAnchor ? children.filter((child) => child.id !== `target`) : children
      setRoot(tree({ ...fx.surface, children: next }))
      if (c.step.scroll) fireEvent.scroll(container.querySelector(`[data-xui-id="${c.step.scroll}"]`)!)
      await frames()
      expect(shown(), c.name).toBe(c.after)
      rect.mockRestore()
    })
  }
})

// VAPP-289: a web font still loading lays the page out in its fallback; the
// swap moves the anchor without anything having moved (the core lays out
// with its fonts loaded), so a Popover that starts open stays open.
describe(`watchAnchor while the fonts load`, () => {
  it(`a font swap re-lays the anchor out and keeps it open; a move after it closes it`, async () => {
    const fx = (fixture as unknown as { overlayAnchor: { surface: NestedNode } }).overlayAnchor
    const fonts = { status: `loading` }
    Object.defineProperty(document, `fonts`, { configurable: true, value: fonts })
    let shift = 0
    const rect = vi.spyOn(HTMLElement.prototype, `getBoundingClientRect`).mockImplementation(() => new DOMRect(shift, 0, 100, 40))
    const shown = () => document.querySelector(`[data-xui-overlay="Popover"]`) !== null
    const frames = () => new Promise((resolve) => setTimeout(resolve, 50))
    try {
      render(() => <ExponentialSurface root={tree(fx.surface)} theme="neutral" id="oa" />)
      await frames()
      expect(shown()).toBe(true)
      shift = 7
      fonts.status = `loaded`
      await frames()
      expect(shown(), `the swap is not a move`).toBe(true)
      shift = 20
      await frames()
      expect(shown(), `a move once the fonts settled closes it`).toBe(false)
    } finally {
      rect.mockRestore()
      delete (document as { fonts?: unknown }).fonts
    }
  })
})
