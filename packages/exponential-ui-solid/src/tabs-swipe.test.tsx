// FEED-200 (React's tabs-swipe.test.tsx): a `swipeable` Tabs pages its panels under a TOUCH
// (fixtures/swipe.json, `@exponential_app/eui/web` tabs-swipe): a swipe past
// half the width settles on the neighbour and fires `change`, a short slow
// one springs back, a mouse drag never pages, a touch that starts in a
// sideways-scrolling child is that child's, RTL mirrors. jsdom has no
// layout: the pager's width is pinned, the clock is stubbed.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { cleanup, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"
import type { SurfaceActionEvent } from "./host"

let now = 0
beforeEach(() => {
  now = 0
  vi.spyOn(performance, `now`).mockImplementation(() => now)
})
afterEach(async () => {
  cleanup()
  vi.restoreAllMocks()
  // The click swallow armed after a swipe is removed by a 0 ms timer.
  await new Promise<void>((resolve) => setTimeout(resolve, 0))
})

const WIDTH = 400

function pointer(type: string, target: EventTarget, x: number, pointerType = `touch`, y = 10) {
  const e = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 })
  Object.defineProperty(e, `pointerId`, { value: 1 })
  Object.defineProperty(e, `pointerType`, { value: pointerType })
  target.dispatchEvent(e)
}

function mount(opts: { variant?: string; rtl?: boolean; wide?: boolean } = {}) {
  const actions: SurfaceActionEvent[] = []
  const child = (id: string) =>
    opts.wide && id === `b`
      ? { id: `pb`, component: `Box`, children: [{ id: `wide`, component: `Box`, children: [{ id: `tb`, component: `Text`, props: { text: `B` } }] }] }
      : { id: `p${id}`, component: `Text`, props: { text: id.toUpperCase() } }
  const root = reduceNested(
    {
      id: `tabs`,
      component: `Tabs`,
      props: { swipeable: true, value: `b`, ...(opts.variant ? { variant: opts.variant } : {}), tabs: [`a`, `b`, `c`].map((v) => ({ label: v.toUpperCase(), value: v })) },
      on: { change: { event: { name: `change` } } },
      children: [`a`, `b`, `c`].map(child),
    } as NestedNode,
    { catalogId: CORE_CATALOG_ID }
  ).root
  const { container } = render(() => <ExponentialSurface root={root} theme="neutral" id="sw" direction={opts.rtl ? `rtl` : `ltr`} host={{ onAction: (e) => void actions.push(e) }} />)
  const pager = container.querySelector(`.xui-Tabs-pager`) as HTMLElement
  Object.defineProperty(pager, `clientWidth`, { value: WIDTH, configurable: true })
  const active = () => container.querySelector(`[role="tabpanel"][data-state="active"]`) as HTMLElement
  const changes = () => actions.filter((a) => a.name === `change`).map((a) => a.context.value)
  return { container, pager, active, changes }
}

/** A drag of `dx` px over `ms` in five steps from `target`. */
function swipe(target: Element, dx: number, ms: number, pointerType = `touch`) {
  pointer(`pointerdown`, target, 200, pointerType)
  for (let k = 1; k <= 5; k++) {
    now += ms / 5
    pointer(`pointermove`, window, 200 + (dx * k) / 5, pointerType)
  }
  pointer(`pointerup`, window, 200 + dx, pointerType)
}

describe(`Tabs.swipeable (web)`, () => {
  it(`draws the active panel's neighbours stacked, inert, one width to either side`, () => {
    const { container } = mount()
    const prev = container.querySelector(`[data-xui-page="prev"]`) as HTMLElement
    const next = container.querySelector(`[data-xui-page="next"]`) as HTMLElement
    expect(prev.textContent).toBe(`A`)
    expect(next.textContent).toBe(`C`)
    expect(prev.hasAttribute(`hidden`)).toBe(false)
    expect(next.getAttribute(`aria-hidden`)).toBe(`true`)
    expect(next.hasAttribute(`inert`)).toBe(true)
    expect(next.style.transform).toBe(`translateX(100%)`)
    expect(prev.style.transform).toBe(`translateX(-100%)`)
    expect(container.querySelector(`[data-xui-part="Tabs/thumb"]`)).not.toBeNull()
  })

  it(`a touch swipe past half the width settles on the next tab and fires change`, () => {
    const { pager, active, changes } = mount()
    const start = active().firstElementChild!
    pointer(`pointerdown`, start, 200)
    now += 300
    pointer(`pointermove`, window, 50)
    expect(active().style.transform).toBe(`translateX(-150px)`)
    expect(active().style.transition).toBe(`none`)
    now += 300
    pointer(`pointermove`, window, -50)
    pointer(`pointerup`, window, -50)
    expect(changes()).toEqual([`c`])
    expect(active().textContent).toBe(`C`)
    expect(active().style.transform).toBe(`none`)
    expect(pager.querySelector(`[data-xui-page="prev"]`)!.textContent).toBe(`B`)
  })

  it(`a fling pages even when it is short`, () => {
    const { active, changes } = mount()
    swipe(active().firstElementChild!, -60, 50)
    expect(changes()).toEqual([`c`])
  })

  it(`a short slow swipe springs back`, () => {
    const { active, changes } = mount()
    swipe(active().firstElementChild!, -100, 1000)
    expect(changes()).toEqual([])
    expect(active().textContent).toBe(`B`)
    expect(active().style.transform).toBe(`none`)
  })

  it(`a mouse drag never pages`, () => {
    const { active, changes } = mount()
    pointer(`pointerdown`, active().firstElementChild!, 200, `mouse`)
    pointer(`pointermove`, window, 0, `mouse`)
    expect(active().style.transform).toBe(`none`)
    pointer(`pointerup`, window, 0, `mouse`)
    expect(changes()).toEqual([])
  })

  it(`a touch that starts in a sideways-scrolling child is that child's`, () => {
    const { container, active, changes } = mount({ wide: true })
    const wide = container.querySelector(`[data-xui-id="wide"]`) as HTMLElement
    wide.style.overflowX = `auto`
    swipe(wide.firstElementChild!, -300, 1000)
    expect(changes()).toEqual([])
    expect(active().textContent).toBe(`B`)
  })

  it(`a vertical touch never pages`, () => {
    const { active, changes } = mount()
    pointer(`pointerdown`, active().firstElementChild!, 200, `touch`, 10)
    pointer(`pointermove`, window, 190, `touch`, 60)
    pointer(`pointermove`, window, -100, `touch`, 60)
    expect(active().style.transform).toBe(`none`)
    pointer(`pointerup`, window, -100, `touch`, 60)
    expect(changes()).toEqual([])
  })

  it(`RTL: the next page sits on the left, a rightward swipe reveals it`, () => {
    const { container, active, changes } = mount({ rtl: true })
    expect((container.querySelector(`[data-xui-page="next"]`) as HTMLElement).style.transform).toBe(`translateX(-100%)`)
    swipe(active().firstElementChild!, 250, 1000)
    expect(changes()).toEqual([`c`])
  })

  it(`the strip variant pages too, its thumb wearing the item recipe`, () => {
    const { container, active, changes } = mount({ variant: `strip` })
    const thumb = container.querySelector(`[data-xui-part="Tabs/thumb"]`) as HTMLElement
    expect(thumb.className).toContain(`xui-Tabs-item`)
    expect(thumb.getAttribute(`data-xs`)).toBe(`selected`)
    swipe(active().firstElementChild!, 250, 1000)
    expect(changes()).toEqual([`a`])
  })

  it(`a non-swipeable Tabs draws no pager, no neighbours, no thumb`, () => {
    const root = reduceNested({ id: `tabs`, component: `Tabs`, props: { value: `b`, tabs: [`a`, `b`, `c`].map((v) => ({ label: v, value: v })) }, children: [`a`, `b`, `c`].map((v) => ({ id: v, component: `Text`, props: { text: v } })) } as NestedNode, { catalogId: CORE_CATALOG_ID }).root
    const { container } = render(() => <ExponentialSurface root={root} theme="neutral" id="ns" />)
    expect(container.querySelector(`.xui-Tabs-pager, [data-xui-page], [data-xui-part="Tabs/thumb"], [data-xui-swipe]`)).toBeNull()
  })
})
