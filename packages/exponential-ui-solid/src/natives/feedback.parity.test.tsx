// FEED-128: DOM parity with React for the feedback natives (Ring, Spinner,
// Skeleton, TreeGuides): every example of catalog-components.json (its
// example × every enum value × both booleans, as React's fixtures.test.tsx
// iterates them), forced states, and dark + rtl.

import { describe, it } from "vitest"
import type { NestedNode } from "@exponential_app/ui"
import componentsFixture from "@exponential_app/ui/fixtures/catalog-components.json"
import { expectDomParity } from "../test/parity"

const MINE = new Set([`Ring`, `Spinner`, `Skeleton`, `TreeGuides`])
const cases = (componentsFixture as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => MINE.has(c.node.component))

const guides: NestedNode = {
  id: `root`,
  component: `Box`,
  children: [
    { id: `g0`, component: `TreeGuides`, props: { depth: 3, elbowAt: 2, tee: true, passThrough: [0, 1] } },
    { id: `g1`, component: `TreeGuides`, props: { depth: 2, elbowAt: 1, passThrough: [0] } },
    { id: `g2`, component: `TreeGuides`, props: { depth: 1 } },
    { id: `g3`, component: `TreeGuides`, props: { depth: 0 } },
  ],
}
const skeletonRows: NestedNode = {
  id: `root`,
  component: `Box`,
  children: [
    { id: `s0`, component: `Skeleton`, props: { rows: 4 } },
    { id: `s1`, component: `Skeleton`, props: { rows: 2, dot: false, rowHeight: 48 } },
  ],
}
const mixed: NestedNode = {
  id: `root`,
  component: `Box`,
  children: [
    { id: `r`, component: `Ring`, props: { value: 0.3, tone: `success`, size: `lg`, label: `30%` } },
    { id: `sp`, component: `Spinner`, props: { size: `sm`, label: `Saving` } },
    { id: `sk`, component: `Skeleton`, props: { width: `40%`, height: `12px`, rounded: true } },
    ...(guides.children ?? []),
    ...(skeletonRows.children ?? []),
  ],
}

describe(`feedback natives: DOM parity with React`, () => {
  for (const c of cases) it(c.name, async () => expectDomParity({ tree: c.node }))
  it(`TreeGuides: elbow, tee, pass-through, empty`, async () => expectDomParity({ tree: guides }))
  it(`Skeleton rows (dot, no dot, row height)`, async () => expectDomParity({ tree: skeletonRows }))
  for (const state of [`hover`, `focus`, `pressed`, `disabled`]) it(`forced state ${state}`, async () => expectDomParity({ tree: mixed, states: [state] }))
  it(`exponential theme, dark, rtl`, async () => expectDomParity({ tree: mixed, theme: `exponential`, mode: `dark`, direction: `rtl` }))
})
