// FEED-128: DOM parity with React for the feedback natives (Ring, Spinner,
// Skeleton, TreeGuides, VoiceWave): every example of catalog-components.json (its
// example × every enum value × both booleans, as React's fixtures.test.tsx
// iterates them), forced states, and dark + rtl.

import { describe, it } from "vitest"
import type { NestedNode } from "@exponential_app/eui"
import componentsFixture from "@exponential_app/eui/fixtures/catalog-components.json"
import { expectDomParity } from "../test/parity"

const MINE = new Set([`Ring`, `Spinner`, `Skeleton`, `TreeGuides`, `VoiceWave`])
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
    { id: `vw`, component: `VoiceWave`, props: { activity: `speaking`, level: 0.4, bands: [0.7, 1, 0.5, 0.8], size: `md`, label: `Voice` }, children: [{ id: `vw-mark`, component: `Icon`, props: { name: `mic` } }] },
    ...(guides.children ?? []),
    ...(skeletonRows.children ?? []),
  ],
}

describe(`feedback natives: DOM parity with React`, () => {
  for (const c of cases) it(c.name, async () => expectDomParity({ tree: c.node }))
  it(`TreeGuides: elbow, tee, pass-through, empty`, async () => expectDomParity({ tree: guides }))
  it(`Skeleton rows (dot, no dot, row height)`, async () => expectDomParity({ tree: skeletonRows }))
  it(`VoiceWave bound to the data model (activity, level, bands)`, async () =>
    expectDomParity({
      tree: { id: `root`, component: `Box`, children: [{ id: `vw`, component: `VoiceWave`, props: { activity: { path: `/voice/activity` }, level: { path: `/voice/level` }, bands: { path: `/voice/bands` }, size: `xl`, label: `Voice` }, children: [{ id: `vw-t`, component: `Text`, props: { text: `V` } }] }] } as NestedNode,
      data: { voice: { activity: `listening`, level: 0.8, bands: [1, 0.2, 0.6] } },
    }))
  // VAPP-162 (round-5 contract §5): the bars meter at every size, still and moving, with an unpainted child.
  for (const size of [`sm`, `md`, `lg`, `xl`])
    for (const activity of [`idle`, `speaking`])
      it(`VoiceWave bars ${size} ${activity}`, async () =>
        expectDomParity({ tree: { id: `vb`, component: `VoiceWave`, props: { variant: `bars`, size, activity, level: 0.6, bands: [0.2, 1, 0.5], label: `Level` } } as NestedNode }))
  it(`VoiceWave bars: a child stays unpainted; bound inputs`, async () =>
    expectDomParity({
      tree: { id: `vb`, component: `VoiceWave`, props: { variant: `bars`, activity: { path: `/a` }, level: { path: `/l` } }, children: [{ id: `vb-t`, component: `Text`, props: { text: `V` } }] } as NestedNode,
      data: { a: `listening`, l: 0.9 },
    }))
  it(`VoiceWave bars, exponential dark rtl`, async () => expectDomParity({ tree: { id: `vb`, component: `VoiceWave`, props: { variant: `bars`, activity: `muted` } } as NestedNode, theme: `exponential`, mode: `dark`, direction: `rtl` }))
  for (const state of [`hover`, `focus`, `pressed`, `disabled`]) it(`forced state ${state}`, async () => expectDomParity({ tree: mixed, states: [state] }))
  it(`exponential theme, dark, rtl`, async () => expectDomParity({ tree: mixed, theme: `exponential`, mode: `dark`, direction: `rtl` }))
})
