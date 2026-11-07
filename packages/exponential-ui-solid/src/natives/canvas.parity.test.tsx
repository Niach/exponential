// Round 8: DOM parity of Canvas (natives/canvas.tsx) with React: every
// catalog-components.json example, a styled box (a style width wins over
// the drawing size), both edges styled (no aspect), a decorative one, a
// pointer one, a bound `shapes`, dark + rtl.

import { describe, it } from "vitest"
import fixture from "@exponential_app/eui/fixtures/catalog-components.json"
import type { NestedNode } from "@exponential_app/eui"
import { expectDomParity } from "../test/parity"

const cases = (fixture as unknown as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => c.node.component === `Canvas`)
const bars = { size: [120, 60], shapes: [{ type: `rect`, position: [30, 40], size: [20, 40] }, { type: `fill`, color: `$color.primary` }, { type: `text`, text: `Mon`, position: [30, 58], size: 9, align: `center` }] }

describe(`DOM parity: canvas`, { timeout: 60_000 }, () => {
  for (const c of cases) it(c.name, async () => expectDomParity({ tree: c.node }))
  it(`styled boxes, decorative, pointer, bound shapes`, async () =>
    expectDomParity({
      tree: {
        id: `root`,
        component: `Stack`,
        children: [
          { id: `c1`, component: `Canvas`, props: { ...bars, label: `Visits` } },
          { id: `c2`, component: `Canvas`, props: { ...bars, fit: `cover` }, style: { width: 200, height: 40 } },
          { id: `c3`, component: `Canvas`, props: { ...bars }, style: { width: 64 } },
          { id: `c4`, component: `Canvas`, props: { ...bars, pointer: true, label: `Pick` }, on: { press: { event: { name: `p` } } } },
          { id: `c5`, component: `Canvas`, props: { size: [10, 10], shapes: { path: `/shapes` } } },
        ],
      } as NestedNode,
      data: { shapes: bars.shapes },
    }))
  it(`dark rtl`, async () => expectDomParity({ tree: { id: `c`, component: `Canvas`, props: { ...bars, label: `Bars` } } as NestedNode, theme: `exponential`, mode: `dark`, direction: `rtl` }))
})
