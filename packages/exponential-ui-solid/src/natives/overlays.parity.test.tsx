// FEED-128: DOM parity with React for the overlays (Dialog, Drawer,
// Popover, Tooltip, Menu, Toast): every example of catalog-components.json
// (its example × every enum value × both booleans, as React's
// fixtures.test.tsx iterates them; the `open: true` ones painted open in
// the overlay layer), triggers and footers, forced states, dark + rtl.
//
// The placement is ASYNC on both sides (floating-ui's `computePosition`):
// each renderer is painted ALONE (two modal focus traps in one document
// would fight), given a few macrotasks to settle, serialised and torn down
// before the other one paints.

import { describe, expect, it } from "vitest"
import type { NestedNode } from "@exponential_app/ui"
import componentsFixture from "@exponential_app/ui/fixtures/catalog-components.json"
import round4 from "@exponential_app/ui/fixtures/round4-contract.json"
import { renderReact, renderSolidSurface, serializeDom, type ParityCase } from "../test/parity"
import { PENDING_NATIVES } from "./index"

const settle = async () => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 5))
}

/** `expectDomParity` with the async placement settled, one renderer at a time. */
async function expectSettledParity(c: ParityCase): Promise<void> {
  const r = await renderReact(c)
  await settle()
  const react = serializeDom(r.root)
  r.cleanup()
  await settle()
  const s = await renderSolidSurface(c)
  await settle()
  const solid = serializeDom(s.root)
  s.cleanup()
  await settle()
  expect(solid).toBe(react)
}

const MINE = new Set([`Dialog`, `Drawer`, `Popover`, `Tooltip`, `Menu`, `Toast`, `AlertDialog`])
const cases = (componentsFixture as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => MINE.has(c.node.component))
/** The AlertDialog macro paints Buttons (the actions lane). */
const needsButton = (c: { node: NestedNode }) => JSON.stringify(c.node).includes(`"AlertDialog"`)
const buttonPending = PENDING_NATIVES.includes(`Button`)

const text = (id: string, t: string): NestedNode => ({ id, component: `Text`, props: { text: t } })

const triggers: NestedNode = {
  id: `root`,
  component: `Box`,
  children: [
    { id: `dlg`, component: `Dialog`, props: { title: `Edit`, open: true }, slots: { trigger: text(`dlg-t`, `Open`), footer: text(`dlg-f`, `Footer`) }, children: [text(`dlg-c`, `Body`)] },
    { id: `pop`, component: `Popover`, props: { side: `top`, open: true }, slots: { trigger: text(`pop-t`, `Pop`) }, children: [text(`pop-c`, `Popover body`)] },
    { id: `pop2`, component: `Popover`, props: { side: `right`, openOn: `hover` }, slots: { trigger: { id: `pop2-t`, component: `Link`, props: { text: `Card`, href: `https://example.com` } } }, children: [text(`pop2-c`, `Card body`)] },
    { id: `menu`, component: `Menu`, props: { icon: `star`, items: [{ label: `One`, value: `1` }] } },
    { id: `menu2`, component: `Menu`, props: { label: `Actions`, icon: `star`, items: [{ label: `One`, value: `1` }] } },
    { id: `menu3`, component: `Menu`, props: { items: [{ label: `One`, value: `1` }] } },
    { id: `tip`, component: `Tooltip`, props: { content: `Hint`, side: `left` }, children: [text(`tip-c`, `Hover`)] },
    { id: `toast-a`, component: `Toast`, props: { title: `Saved`, type: `success`, duration: 0 } },
    { id: `toast-b`, component: `Toast`, props: { title: `Heads up`, description: `Two`, type: `warning`, dismissible: false, duration: 0 } },
  ],
}

const drawers = (side: string): NestedNode => ({
  id: `root`,
  component: `Box`,
  children: [{ id: `drw`, component: `Drawer`, props: { title: `Filters`, description: `Narrow it down`, side, open: true }, slots: { footer: text(`drw-f`, `Apply`) }, children: [text(`drw-c`, `Body`)] }],
})

const bare: NestedNode = {
  id: `root`,
  component: `Box`,
  children: [
    { id: `d0`, component: `Dialog`, props: { open: true }, children: [text(`d0-c`, `No title`)] },
    { id: `d1`, component: `Drawer`, props: { open: true, side: `left`, dismissible: false }, children: [text(`d1-c`, `Pinned`)] },
  ],
}

const everything: NestedNode = {
  id: `root`,
  component: `Box`,
  children: [
    { id: `dlg`, component: `Dialog`, props: { title: `Edit`, description: `Change it`, open: true }, children: [text(`dlg-c`, `Body`)] },
    { id: `pop`, component: `Popover`, props: { open: true }, children: [text(`pop-c`, `Popover body`)] },
    { id: `menu`, component: `Menu`, props: { label: `More`, items: [{ label: `One`, value: `1` }] } },
    { id: `cm`, component: `Menu`, props: { openOn: `contextmenu`, items: [{ label: `One`, value: `1` }] }, children: [text(`cm-c`, `Area`)] },
    { id: `tip`, component: `Tooltip`, props: { content: `Hint` }, children: [text(`tip-c`, `Hover`)] },
    { id: `toast`, component: `Toast`, props: { title: `Saved`, description: `All good`, actionLabel: `Undo`, duration: 0 } },
  ],
}

describe(`overlays: DOM parity with React`, () => {
  for (const c of cases) it.skipIf(needsButton(c) && buttonPending)(c.name, async () => expectSettledParity({ tree: c.node }))
  it(`triggers, footers, the default menu buttons, a hover card, toasts`, async () => expectSettledParity({ tree: triggers }))
  for (const side of [`top`, `right`, `bottom`, `left`]) it(`Drawer open on the ${side} with a description and a footer`, async () => expectSettledParity({ tree: drawers(side) }))
  it(`a Dialog without a title (sr-only title) and a non-dismissible Drawer`, async () => expectSettledParity({ tree: bare }))
  for (const state of [`hover`, `focus`, `pressed`, `open`, `disabled`]) it(`forced state ${state}`, async () => expectSettledParity({ tree: everything, states: [state] }))
  for (const c of round4.closeOnSubmit) it.skipIf(buttonPending)(`round 4 tree, open at mount: ${c.name}`, async () => expectSettledParity({ tree: c.input as unknown as NestedNode, data: c.data }))
  it(`exponential theme, dark, rtl`, async () => expectSettledParity({ tree: everything, theme: `exponential`, mode: `dark`, direction: `rtl` }))
})
