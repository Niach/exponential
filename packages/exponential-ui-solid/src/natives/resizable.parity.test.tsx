// FEED-128: Resizable DOM parity with React (src/test/parity.ts): every
// catalog example (each enum value × both booleans), three panels with
// limits, normalised sizes, a bound `sizes`, forced states, dark + rtl.

import { describe, it } from "vitest"
import type { NestedNode } from "@exponential_app/ui"
import components from "@exponential_app/ui/fixtures/catalog-components.json"
import { expectDomParity } from "../test/parity"

type Case = { name: string; node: NestedNode }
const examples = (components as { cases: Case[] }).cases.filter((c) => c.node.component === `Resizable`)

const three = (props: Record<string, unknown>): NestedNode => ({
  id: `rz`,
  component: `Resizable`,
  props: { sizes: [20, 30, 50], panels: [{ min: 10, max: 60 }, { min: 20 }, {}], handle: true, ...props },
  style: { width: 401, height: 80 },
  children: [0, 1, 2].map((i) => ({ id: `p${i}`, component: `Text`, props: { text: `panel ${i}` } })),
})

describe(`Resizable DOM parity with React`, () => {
  for (const c of examples) it(c.name, async () => expectDomParity({ tree: c.node }))
  it(`three panels with limits, horizontal`, async () => expectDomParity({ tree: three({}) }))
  it(`three panels, vertical, no grip`, async () => expectDomParity({ tree: three({ direction: `vertical`, handle: false }) }))
  it(`sizes that need normalising`, async () => expectDomParity({ tree: three({ sizes: [1, 1] }) }))
  it(`a bound sizes`, async () => expectDomParity({ tree: three({ sizes: { path: `/s` } }), data: { s: [50, 25, 25] } }))
  for (const s of [`hover`, `focus`, `pressed`, `checked`, `open`, `disabled`, `invalid`]) it(`forced state ${s}`, async () => expectDomParity({ tree: three({}), states: [s] }))
  it(`dark + rtl in the exponential theme`, async () => expectDomParity({ tree: three({}), theme: `exponential`, mode: `dark`, direction: `rtl` }))
})
