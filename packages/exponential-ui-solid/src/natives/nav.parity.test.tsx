// FEED-128: Tabs, Segmented and Accordion DOM parity with React
// (src/test/parity.ts): every catalog example (each enum value × both
// booleans, as React's fixtures.test.tsx iterates them), icons/counts/
// disabled items, closed panels, forced states, dark + rtl.

import { describe, it } from "vitest"
import type { NestedNode } from "@exponential_app/ui"
import components from "@exponential_app/ui/fixtures/catalog-components.json"
import { expectDomParity } from "../test/parity"

type Case = { name: string; node: NestedNode }
const MINE = new Set([`Tabs`, `Segmented`, `Accordion`])
const examples = (components as { cases: Case[] }).cases.filter((c) => MINE.has(c.node.component))

const tabs: NestedNode = {
  id: `tabs`,
  component: `Tabs`,
  props: { tabs: [{ label: `Issue`, value: `issue`, icon: `star`, count: 3 }, { label: `Run`, value: `run` }, { label: `Logs`, value: `logs`, count: 1200 }], value: `run`, closable: `hover`, addable: true, fill: true },
  children: [1, 2, 3].map((i) => ({ id: `p${i}`, component: `Text`, props: { text: `panel ${i}` } })),
}
const segmented = (variant: string, type = `single`, value: unknown = `b`): NestedNode => ({
  id: `seg`,
  component: `Segmented`,
  props: { variant, type, value, items: [{ label: `A`, value: `a`, icon: `star` }, { label: `B`, value: `b` }, { label: ``, value: `c`, icon: `star` }, { label: `D`, value: `d`, disabled: true }] },
})
const accordion = (type: string, value: unknown): NestedNode => ({
  id: `acc`,
  component: `Accordion`,
  props: { type, value, items: [{ title: `First`, value: `1`, count: 2 }, { title: `Second`, value: `2` }, { title: `Third`, value: `3` }] },
  children: [1, 2, 3].map((i) => ({ id: `b${i}`, component: `Text`, props: { text: `body ${i}` } })),
})

describe(`nav DOM parity with React`, () => {
  for (const c of examples) it(c.name, async () => expectDomParity({ tree: c.node }))
  it(`Tabs: icons, counts, closable hover, addable, fill, the middle tab active`, async () => expectDomParity({ tree: tabs }))
  it(`Tabs: a bound value`, async () => expectDomParity({ tree: { ...tabs, props: { ...tabs.props, value: { path: `/tab` } } } as NestedNode, data: { tab: `logs` } }))
  it(`Tabs: no value = the first tab`, async () => expectDomParity({ tree: { ...tabs, props: { ...tabs.props, value: undefined, closable: `always` } } as NestedNode }))
  for (const variant of [`segmented`, `toggles`, `outline`, `bar`])
    for (const type of [`single`, `multiple`]) it(`Segmented ${variant} ${type}: icons, an icon-only item, a disabled item`, async () => expectDomParity({ tree: segmented(variant, type, type === `multiple` ? `a,c` : `b`) }))
  it(`Segmented multiple bound to an array`, async () => expectDomParity({ tree: segmented(`segmented`, `multiple`, { path: `/v` }), data: { v: [`b`, `d`] } }))
  // VAPP-162 (round-5 contract §3): every variant stacked; the vertical bar = the rail with count badges.
  for (const variant of [`segmented`, `toggles`, `outline`, `bar`]) it(`Segmented ${variant} vertical`, async () => expectDomParity({ tree: { ...segmented(variant), props: { ...segmented(variant).props, orientation: `vertical` } } as NestedNode }))
  const rail = (extra: Record<string, unknown> = {}): NestedNode => ({
    id: `rail`,
    component: `Segmented`,
    props: {
      variant: `bar`,
      orientation: `vertical`,
      value: `chats`,
      items: [
        { label: `Chats`, value: `chats`, icon: `star`, count: 3 },
        { label: `Calls`, value: `calls`, icon: `star`, count: 120 },
        { label: `Settings`, value: `settings`, icon: `star`, count: 0 },
        { value: `bare`, icon: `star`, count: 7 },
        { label: `Off`, value: `off`, icon: `star`, count: 2, disabled: true },
      ],
      ...extra,
    },
  })
  it(`Segmented rail with counts (99+, none at 0, icon-only, disabled)`, async () => expectDomParity({ tree: rail() }))
  it(`Segmented horizontal bar with counts`, async () => expectDomParity({ tree: rail({ orientation: undefined }) }))
  it(`Segmented rail: a bound count`, async () => expectDomParity({ tree: rail({ items: [{ label: `Chats`, value: `chats`, icon: `star`, count: { path: `/n` } }] }), data: { n: 42 } }))
  it(`Segmented rail with counts, dark + rtl`, async () => expectDomParity({ tree: rail(), theme: `exponential`, mode: `dark`, direction: `rtl` }))
  it(`Accordion single: one open, the others closed`, async () => expectDomParity({ tree: accordion(`single`, `2`) }))
  it(`Accordion single: all closed`, async () => expectDomParity({ tree: accordion(`single`, ``) }))
  it(`Accordion multiple: two open`, async () => expectDomParity({ tree: accordion(`multiple`, `1,3`) }))
  it(`Accordion multiple bound to an array`, async () => expectDomParity({ tree: accordion(`multiple`, { path: `/v` }), data: { v: [`2`] } }))
  for (const s of [`hover`, `focus`, `pressed`, `checked`, `open`, `disabled`, `invalid`]) {
    it(`forced state ${s}: Tabs, Segmented, Accordion`, async () => expectDomParity({ tree: { id: `root`, component: `Box`, children: [tabs, segmented(`outline`), accordion(`single`, `1`)] }, states: [s] }))
  }
  it(`dark + rtl in the exponential theme`, async () =>
    expectDomParity({ tree: { id: `root`, component: `Box`, children: [tabs, segmented(`segmented`), segmented(`bar`), accordion(`multiple`, `1`)].map((n, i) => ({ ...n, id: `${n.id}${i}` })) as NestedNode[] }, theme: `exponential`, mode: `dark`, direction: `rtl` }))
  it(`a node with its own rtl direction (rootProps dir)`, async () =>
    expectDomParity({ tree: { id: `root`, component: `Box`, children: [{ ...tabs, style: { direction: `rtl` } }, { ...segmented(`segmented`), style: { direction: `rtl` } }, { ...accordion(`single`, `1`), style: { direction: `rtl` } }] as NestedNode[] } }))
})
