// #FEED-232: the CountBadge native: the label rule (`99+`, `max`), nothing
// at 0 or below, the bound count, the tone on the root recipe.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"

afterEach(cleanup)

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root
const el = (container: HTMLElement, id: string) => container.querySelector<HTMLElement>(`[data-xui-id="${id}"]`)

describe(`CountBadge`, () => {
  it(`prints the label, caps it and renders nothing at 0 or below`, () => {
    const root = tree({
      id: `root`,
      component: `Box`,
      children: [
        { id: `a`, component: `CountBadge`, props: { count: 3, tone: `attention` } },
        { id: `b`, component: `CountBadge`, props: { count: 120 } },
        { id: `c`, component: `CountBadge`, props: { count: 12, max: 9 } },
        { id: `z`, component: `CountBadge`, props: { count: 0, tone: `attention` } },
        { id: `n`, component: `CountBadge`, props: { count: -3 } },
        { id: `d`, component: `CountBadge`, props: { count: { "@path": `/unread` } } },
      ],
    } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface root={root} data={{ unread: 7 }} theme="neutral" id="cb" />)
    expect(el(container, `a`)?.textContent).toBe(`3`)
    expect(el(container, `b`)?.textContent).toBe(`99+`)
    expect(el(container, `c`)?.textContent).toBe(`9+`)
    expect(el(container, `d`)?.textContent).toBe(`7`)
    expect(el(container, `z`)).toBeNull()
    expect(el(container, `n`)).toBeNull()
    expect(el(container, `a`)?.dataset.xuiC).toBe(`CountBadge`)
  })
})
