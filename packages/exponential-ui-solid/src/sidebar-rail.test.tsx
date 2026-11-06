// #VAPP-277 (React's sidebar-rail.test.tsx): the Solid half of the Sidebar
// rail: ONE tab stop per rail row, the Row's keyboard focus opens its
// tooltip, and the tooltip's logical side resolves against the direction.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

const shell = (side: `left` | `right`): NestedNode => ({
  id: `shell`,
  component: `Sidebar`,
  props: { rail: true, collapsed: true, collapseBelow: `never`, side },
  slots: { sidebar: { id: `nav`, component: `Stack`, children: [{ id: `inbox`, component: `Row`, props: { title: `Inbox`, icon: `nav-inbox`, meta: `3`, pressable: true } }] } },
  children: [{ id: `main`, component: `Text`, props: { text: `Pick a board.` } }],
})
const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root
const tick = () => new Promise((r) => setTimeout(r, 0))

describe(`Sidebar rail (#VAPP-277)`, () => {
  it(`one tab stop per rail row: the Row, named by its title and count`, () => {
    const { container } = render(() => <ExponentialSurface root={tree(shell(`left`))} theme="neutral" id="rail" />)
    const stops = [...container.querySelectorAll<HTMLElement>(`[tabindex]`)].filter((el) => el.tabIndex >= 0)
    expect(stops.map((el) => el.getAttribute(`aria-label`))).toEqual([`Inbox, 3`])
    expect(container.querySelector(`[data-xui-part="Tooltip/anchor"]`)?.hasAttribute(`tabindex`)).toBe(false)
  })

  it(`the Row's keyboard focus opens its tooltip, blur closes it`, async () => {
    const { container } = render(() => <ExponentialSurface root={tree(shell(`left`))} theme="neutral" id="rail" />)
    const row = container.querySelector<HTMLElement>(`[aria-label="Inbox, 3"]`)!
    fireEvent.focus(row)
    await tick()
    expect(document.querySelector(`[data-xui-overlay="Tooltip"]`)?.textContent).toContain(`Inbox`)
    fireEvent.blur(row)
    await tick()
    expect(document.querySelector(`[data-xui-overlay="Tooltip"]`)?.getAttribute(`data-state`) ?? `closed`).toBe(`closed`)
  })

  it(`the tooltip points into the content, mirrored under RTL`, async () => {
    const sides: string[] = []
    for (const [side, direction] of [[`left`, `ltr`], [`right`, `ltr`], [`left`, `rtl`], [`right`, `rtl`]] as const) {
      const { container } = render(() => <ExponentialSurface root={tree(shell(side))} theme="neutral" id={`rail-${side}-${direction}`} direction={direction} />)
      const row = container.querySelector<HTMLElement>(`[aria-label="Inbox, 3"]`)!
      fireEvent.focus(row)
      await tick()
      sides.push(document.querySelector(`[data-xui-overlay="Tooltip"]`)?.getAttribute(`data-side`) ?? `none`)
      cleanup()
    }
    expect(sides).toEqual([`right`, `left`, `left`, `right`])
  })

  it(`a Tree on the rail (#VAPP-285): every row a button tile, one tab stop each, no tree roles`, () => {
    const files = { id: `files`, component: `Tree`, props: { label: `Files`, expanded: [], items: [{ id: `src`, title: `src`, children: [{ id: `src/a.ts`, title: `a.ts` }] }, { id: `README.md`, title: `README.md` }] } }
    const node: NestedNode = { ...shell(`left`), slots: { sidebar: files as NestedNode } }
    const { container } = render(() => <ExponentialSurface root={tree(node)} theme="neutral" id="rail-tree" />)
    const stops = [...container.querySelectorAll<HTMLElement>(`[tabindex]`)].filter((el) => el.tabIndex >= 0)
    expect(stops.map((el) => [el.getAttribute(`role`), el.getAttribute(`aria-label`), el.hasAttribute(`aria-expanded`)])).toEqual([[`button`, `src`, false], [`button`, `a.ts`, false], [`button`, `README.md`, false]])
    expect(container.querySelector(`[role="tree"], [role="treeitem"]`)).toBeNull()
  })
})
