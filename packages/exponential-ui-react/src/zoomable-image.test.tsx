// EXP-1586: the React ZoomableImage (the native and its primitive twin) on
// the shared controller: every gesture lands on the core's view (src/zoom.ts
// is the oracle here) — ⌘/ctrl-wheel about the cursor, a plain wheel that
// pans only with room, the double click, the keys (←/→ left to the host), the
// pill's − / + / fit, THE Menu of presets, the single tap at rest, a resize, a
// new src — and the `zoom` / `tap` events.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested, zoomDoubleTap, zoomResize, zoomRestView, zoomScroll, zoomStep, zoomTo, zoomWheel, type NestedNode, type ZoomFrame, type ZoomView } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"
import type { SurfaceActionEvent } from "./host"
import { ZoomableImage } from "./primitives"

let size = { width: 600, height: 400 }
const observers: (() => void)[] = []
class RO {
  constructor(private cb: () => void) {}
  observe() {
    observers.push(() => this.cb())
  }
  unobserve() {}
  disconnect() {}
}
const realRO = globalThis.ResizeObserver

beforeEach(() => {
  size = { width: 600, height: 400 }
  observers.length = 0
  Object.defineProperty(HTMLElement.prototype, `clientWidth`, { configurable: true, get: () => size.width })
  Object.defineProperty(HTMLElement.prototype, `clientHeight`, { configurable: true, get: () => size.height })
  ;(globalThis as { ResizeObserver: unknown }).ResizeObserver = RO
})
afterEach(() => {
  cleanup()
  vi.useRealTimers()
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth
  delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight
  ;(globalThis as { ResizeObserver: unknown }).ResizeObserver = realRO
})

const NATURAL = { width: 1200, height: 800 }
const frame = (viewport = size): ZoomFrame => ({ natural: NATURAL, viewport })
const transformOf = (v: ZoomView) => `translate(${v.x}px, ${v.y}px) scale(${v.scale})`
const tree = (props: Record<string, unknown> = {}) =>
  reduceNested({ id: `z`, component: `ZoomableImage`, props: { src: `https://example.com/a.png`, alt: `Shot`, naturalWidth: 1200, naturalHeight: 800, controls: `always`, ...props }, on: { zoom: { event: { name: `zoomed` } }, tap: { event: { name: `tapped` } } } } as unknown as NestedNode, { catalogId: CORE_CATALOG_ID }).root

function mount(props: Record<string, unknown> = {}) {
  const actions: SurfaceActionEvent[] = []
  const r = render(<ExponentialSurface root={tree(props)} theme="neutral" id="zs" host={{ onAction: (e) => void actions.push(e) }} />)
  const root = r.container.querySelector<HTMLElement>(`[data-xui-id="z"]`)!
  const img = r.container.querySelector<HTMLImageElement>(`[data-xui-part="ZoomableImage/image"]`)!
  // The picture loads (jsdom never fetches): the placeholder gives way to the pill.
  fireEvent.load(img)
  const button = (part: string) => r.container.querySelector<HTMLButtonElement>(`[data-xui-part="ZoomableImage/${part}"]`)!
  return { ...r, actions, root, img, button }
}

const click = (el: Element, x: number, y: number) => {
  fireEvent.pointerDown(el, { pointerId: 1, pointerType: `mouse`, button: 0, clientX: x, clientY: y })
  fireEvent.pointerUp(el, { pointerId: 1, pointerType: `mouse`, button: 0, clientX: x, clientY: y })
}

describe(`ZoomableImage native`, () => {
  it(`paints the parts, opens at the rest view, the pill at its ends`, () => {
    const { root, img, button } = mount()
    expect(root.getAttribute(`role`)).toBe(`group`)
    expect(root.getAttribute(`aria-label`)).toBe(`Shot`)
    expect(root.getAttribute(`tabindex`)).toBe(`0`)
    expect(root.style.aspectRatio).toBe(`1.5`)
    expect(img.getAttribute(`alt`)).toBe(`Shot`)
    expect(img.style.transformOrigin).toBe(`0 0`)
    expect(img.style.transform).toBe(transformOf(zoomRestView(frame())))
    expect(button(`percent`).textContent).toBe(`50%`)
    expect(button(`percent`).getAttribute(`aria-label`)).toBe(`Zoom level`)
    expect(button(`zoomOut`).disabled).toBe(true)
    expect(button(`zoomOut`).getAttribute(`data-xs`)).toBe(`disabled`)
    expect(button(`zoomIn`).disabled).toBe(false)
    expect(button(`fit`).getAttribute(`aria-pressed`)).toBe(`true`)
    expect(button(`fit`).getAttribute(`data-xs`)).toBe(`selected`)
    expect(root.style.cursor).toBe(`zoom-in`)
    expect(root.style.touchAction).toBe(`pan-x pan-y`)
  })

  it(`a tall picture opens at fit-width on a square box; controls none draws no pill`, () => {
    const { root, container } = mount({ naturalWidth: 390, naturalHeight: 3000, controls: `none` })
    expect(root.style.aspectRatio).toBe(`1`)
    expect(container.querySelector(`[data-xui-part="ZoomableImage/pill"]`)).toBeNull()
  })

  it(`until the picture loads: the placeholder (glyph + alt), the picture hidden, NO pill`, () => {
    const r = render(<ExponentialSurface root={tree()} theme="neutral" id="zp" />)
    const img = r.container.querySelector<HTMLImageElement>(`[data-xui-part="ZoomableImage/image"]`)!
    const placeholder = () => r.container.querySelector<HTMLElement>(`.xui-zoom-placeholder`)
    expect(img.style.opacity).toBe(`0`)
    expect(placeholder()!.getAttribute(`aria-hidden`)).toBe(`true`)
    expect(placeholder()!.querySelector(`svg`)).not.toBeNull()
    expect(placeholder()!.textContent).toBe(`Shot`)
    expect(placeholder()!.style.borderRadius).toBe(`8px`)
    expect(r.container.querySelector(`[data-xui-part="ZoomableImage/pill"]`)).toBeNull()
    fireEvent.load(img)
    expect(img.style.opacity).toBe(``)
    expect(placeholder()).toBeNull()
    expect(r.container.querySelector(`[data-xui-part="ZoomableImage/pill"]`)).not.toBeNull()
  })

  it(`⌘/ctrl-wheel zooms about the cursor (never the page) and settles into one zoom event`, () => {
    vi.useFakeTimers()
    const { root, img, actions } = mount()
    const wheel = new WheelEvent(`wheel`, { deltaY: -200, ctrlKey: true, clientX: 150, clientY: 100, bubbles: true, cancelable: true })
    act(() => void root.dispatchEvent(wheel))
    expect(wheel.defaultPrevented).toBe(true)
    const expected = zoomWheel(zoomRestView(frame()), -200, `pixel`, { x: 150, y: 100 }, frame())
    expect(img.style.transform).toBe(transformOf(expected))
    expect(actions).toEqual([])
    act(() => void vi.advanceTimersByTime(200))
    expect(actions.map((a) => [a.event, a.name, a.payload])).toEqual([[`zoom`, `zoomed`, { scale: expected.scale, atFit: false }]])
  })

  it(`a plain wheel pans only with room; without room it reaches the page`, () => {
    const { root, img } = mount()
    const idle = new WheelEvent(`wheel`, { deltaY: 40, bubbles: true, cancelable: true })
    act(() => void root.dispatchEvent(idle))
    expect(idle.defaultPrevented).toBe(false)
    fireEvent.keyDown(root, { key: `1` })
    const at100 = zoomTo(zoomRestView(frame()), 1, { x: 300, y: 200 }, frame())
    expect(img.style.transform).toBe(transformOf(at100))
    const pan = new WheelEvent(`wheel`, { deltaX: 30, deltaY: 40, bubbles: true, cancelable: true })
    act(() => void root.dispatchEvent(pan))
    expect(pan.defaultPrevented).toBe(true)
    expect(img.style.transform).toBe(transformOf(zoomScroll(at100, 30, 40, frame())))
    expect(root.style.cursor).toBe(`grab`)
    expect(root.style.touchAction).toBe(`none`)
  })

  it(`a double click zooms in about the point, another goes back to rest`, () => {
    const { root, img, actions } = mount()
    click(root, 120, 80)
    click(root, 120, 80)
    const zoomed = zoomDoubleTap(zoomRestView(frame()), { x: 120, y: 80 }, frame())
    expect(zoomed.scale).toBe(1)
    expect(img.style.transform).toBe(transformOf(zoomed))
    expect(actions.map((a) => a.payload)).toEqual([{ scale: 1, atFit: false }])
    click(root, 10, 10)
    click(root, 10, 10)
    expect(img.style.transform).toBe(transformOf(zoomRestView(frame())))
    expect(actions.map((a) => a.payload)).toEqual([{ scale: 1, atFit: false }, { scale: 0.5, atFit: true }])
  })

  it(`a single click at rest fires tap after the double-tap window`, () => {
    vi.useFakeTimers()
    const { root, actions } = mount()
    click(root, 50, 50)
    expect(actions).toEqual([])
    act(() => void vi.advanceTimersByTime(350))
    expect(actions.map((a) => [a.event, a.name])).toEqual([[`tap`, `tapped`]])
  })

  it(`keys: + steps up, − down, 0 = rest, 1 = 100%; ←/→ stay the host's`, () => {
    const { root, img } = mount()
    const rest = zoomRestView(frame())
    fireEvent.keyDown(root, { key: `+` })
    const up = zoomStep(rest, 1, frame())
    expect(img.style.transform).toBe(transformOf(up))
    fireEvent.keyDown(root, { key: `-` })
    expect(img.style.transform).toBe(transformOf(zoomStep(up, -1, frame())))
    fireEvent.keyDown(root, { key: `1` })
    expect(img.style.transform).toBe(transformOf(zoomTo(zoomStep(up, -1, frame()), 1, { x: 300, y: 200 }, frame())))
    fireEvent.keyDown(root, { key: `0` })
    expect(img.style.transform).toBe(transformOf(rest))
    const arrow = new KeyboardEvent(`keydown`, { key: `ArrowRight`, bubbles: true, cancelable: true })
    root.dispatchEvent(arrow)
    expect(arrow.defaultPrevented).toBe(false)
  })

  it(`the pill: + / − step about the centre, fit goes back to rest`, () => {
    const { img, button } = mount()
    const rest = zoomRestView(frame())
    fireEvent.click(button(`zoomIn`))
    const up = zoomStep(rest, 1, frame())
    expect(img.style.transform).toBe(transformOf(up))
    expect(button(`percent`).textContent).toBe(`67%`)
    expect(button(`zoomOut`).disabled).toBe(false)
    expect(button(`fit`).getAttribute(`aria-pressed`)).toBe(`false`)
    fireEvent.click(button(`zoomOut`))
    expect(img.style.transform).toBe(transformOf(zoomStep(up, -1, frame())))
    fireEvent.click(button(`zoomIn`))
    fireEvent.click(button(`fit`))
    expect(img.style.transform).toBe(transformOf(rest))
  })

  it(`the percent opens THE Menu of presets; a row zooms to it`, () => {
    const { img, button } = mount()
    fireEvent.keyDown(button(`percent`), { key: `Enter` })
    const content = document.querySelector<HTMLElement>(`[data-xui-part="Menu/content"]`)!
    expect(content).not.toBeNull()
    const rows = [...content.querySelectorAll<HTMLElement>(`[role="menuitemcheckbox"]`)]
    expect(rows.map((r) => r.textContent)).toEqual([`Fit`, `50%`, `100%`, `200%`, `400%`])
    expect(rows.map((r) => r.getAttribute(`aria-checked`))).toEqual([`true`, `false`, `false`, `false`, `false`])
    fireEvent.click(rows[3]!)
    expect(img.style.transform).toBe(transformOf(zoomTo(zoomRestView(frame()), 2, { x: 300, y: 200 }, frame())))
    expect(button(`percent`).textContent).toBe(`200%`)
    expect(document.querySelector(`[data-xui-part="Menu/content"]`)).toBeNull()
  })

  it(`a resize at rest re-fits; zoomed it keeps the scale about the centre`, () => {
    const { root, img } = mount()
    size = { width: 300, height: 200 }
    act(() => observers.forEach((o) => o()))
    expect(img.style.transform).toBe(transformOf(zoomRestView(frame())))
    fireEvent.keyDown(root, { key: `1` })
    const zoomed = zoomTo(zoomRestView(frame()), 1, { x: 150, y: 100 }, frame())
    expect(img.style.transform).toBe(transformOf(zoomed))
    size = { width: 600, height: 400 }
    act(() => observers.forEach((o) => o()))
    expect(img.style.transform).toBe(transformOf(zoomResize(zoomed, { width: 300, height: 200 }, frame())))
  })

  it(`a new src goes back to the rest view at once`, () => {
    const { root, img, rerender } = mount()
    fireEvent.keyDown(root, { key: `1` })
    expect(img.style.transform).not.toBe(transformOf(zoomRestView(frame())))
    rerender(<ExponentialSurface root={tree({ src: `https://example.com/b.png` })} theme="neutral" id="zs" />)
    expect(img.style.transform).toBe(transformOf(zoomRestView(frame())))
    expect(img.style.transition).toBe(``)
  })

  it(`a bound scale opens there and a zoom writes it back`, () => {
    const r = render(
      <ExponentialSurface
        root={reduceNested({ id: `z`, component: `ZoomableImage`, props: { src: `https://example.com/a.png`, alt: `Shot`, naturalWidth: 1200, naturalHeight: 800, scale: { "@path": `/scale` } } } as unknown as NestedNode, { catalogId: CORE_CATALOG_ID }).root}
        data={{ scale: 2 }}
        theme="neutral"
        id="zb"
      />
    )
    const img = r.container.querySelector<HTMLImageElement>(`[data-xui-part="ZoomableImage/image"]`)!
    fireEvent.load(img)
    expect(img.style.transform).toBe(transformOf(zoomTo(zoomRestView(frame()), 2, { x: 300, y: 200 }, frame())))
    fireEvent.keyDown(r.container.querySelector(`[data-xui-id="z"]`)!, { key: `0` })
    expect(img.style.transform).toBe(transformOf(zoomRestView(frame())))
    expect(r.container.querySelector(`[data-xui-part="ZoomableImage/percent"]`)!.textContent).toBe(`50%`)
  })

  it(`controls auto hides the pill after autoHideMs, a pointer move shows it again`, () => {
    vi.useFakeTimers()
    const { root, container } = mount({ controls: `auto` })
    const pill = () => container.querySelector<HTMLElement>(`[data-xui-part="ZoomableImage/pill"]`)!
    expect(pill().getAttribute(`data-state`)).toBe(`open`)
    act(() => void vi.advanceTimersByTime(2100))
    expect(pill().getAttribute(`data-state`)).toBe(`closed`)
    expect(pill().style.opacity).toBe(`0`)
    fireEvent.pointerMove(root, { pointerId: 1, pointerType: `mouse`, clientX: 5, clientY: 5 })
    expect(pill().getAttribute(`data-state`)).toBe(`open`)
  })
})

describe(`ZoomableImage primitive`, () => {
  it(`the same gestures outside a surface, with onZoom / onScaleChange / onTap and string overrides`, () => {
    vi.useFakeTimers()
    const zooms: unknown[] = []
    const scales: number[] = []
    const taps: number[] = []
    render(<ZoomableImage src="https://example.com/a.png" alt="Shot" naturalWidth={1200} naturalHeight={800} controls="always" onZoom={(z) => zooms.push(z)} onScaleChange={(s) => scales.push(s)} onTap={() => taps.push(1)} strings={{ zoomIn: `Vergrößern` }} />)
    const root = document.querySelector<HTMLElement>(`[data-slot="zoomable-image"]`)!
    const img = document.querySelector<HTMLImageElement>(`[data-slot="zoomable-image-image"]`)!
    expect(root.style.aspectRatio).toBe(`1.5`)
    expect(img.style.transform).toBe(transformOf(zoomRestView(frame())))
    fireEvent.click(screen.getByRole(`button`, { name: `Vergrößern` }))
    expect(img.style.transform).toBe(transformOf(zoomStep(zoomRestView(frame()), 1, frame())))
    expect(screen.getByRole(`button`, { name: `Zoom level` }).textContent).toBe(`67%`)
    fireEvent.click(screen.getByRole(`button`, { name: `Zoom to fit` }))
    expect(scales).toEqual([0.67, 0.5])
    expect(zooms).toEqual([{ scale: 0.67, atFit: false }, { scale: 0.5, atFit: true }])
    click(root, 30, 30)
    act(() => void vi.advanceTimersByTime(350))
    expect(taps).toEqual([1])
  })
})
