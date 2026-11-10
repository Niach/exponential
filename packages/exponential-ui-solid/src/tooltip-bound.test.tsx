// #EXP-1629 (React's tooltip-bound.test.tsx): Tooltip `content` is
// bindable; the open tooltip shows the bound text.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

const node: NestedNode = { id: `tip`, component: `Tooltip`, props: { content: { "@path": `/reason` } }, children: [{ id: `label`, component: `Text`, props: { text: `Reconnect` } }] }
const tick = () => new Promise((r) => setTimeout(r, 0))

describe(`Tooltip bound content (#EXP-1629)`, () => {
  it(`validates and shows the bound hint`, async () => {
    const reduced = reduceNested(node, { catalogId: CORE_CATALOG_ID })
    expect(reduced.issues).toEqual([])
    const { container } = render(() => <ExponentialSurface root={reduced.root} data={{ reason: `Token expired` }} theme="neutral" id="tb" />)
    fireEvent.focusIn(container.querySelector<HTMLElement>(`[data-xui-part="Tooltip/anchor"]`)!)
    await tick()
    expect(document.querySelector(`[data-xui-overlay="Tooltip"]`)?.textContent).toContain(`Token expired`)
  })
})
