// FEED-128: Button, Link and Toggle DOM parity with React
// (src/test/parity.ts): every catalog example (each enum value × both
// booleans), the pending frame past PENDING_DELAY_MS, icons, a denied link,
// forced states, dark + rtl.

import { describe, expect, it } from "vitest"
import { act } from "react"
import { PENDING_DELAY_MS } from "@exponential-at/ui"
import type { NestedNode } from "@exponential-at/ui"
import components from "@exponential-at/ui/fixtures/catalog-components.json"
import { expectDomParity, renderReact, renderSolidSurface, serializeDom, type ParityCase } from "../test/parity"

type Case = { name: string; node: NestedNode }
const MINE = new Set([`Button`, `Link`, `Toggle`])
const examples = (components as { cases: Case[] }).cases.filter((c) => MINE.has(c.node.component))

/** Parity of the frame past the pending delay (what a person sees). */
async function expectPendingParity(c: ParityCase): Promise<void> {
  const r = await renderReact(c)
  const s = await renderSolidSurface(c)
  try {
    await act(async () => {
      await new Promise((done) => setTimeout(done, PENDING_DELAY_MS + 50))
    })
    expect(serializeDom(s.root)).toBe(serializeDom(r.root))
  } finally {
    r.cleanup()
    s.cleanup()
  }
}

const bar: NestedNode = {
  id: `root`,
  component: `Box`,
  children: [
    { id: `b1`, component: `Button`, props: { label: `Save`, icon: `star`, variant: `secondary` } },
    { id: `b2`, component: `Button`, props: { label: `Star`, icon: `star`, size: `icon` } },
    { id: `b3`, component: `Button`, props: { label: `Off`, disabled: true, variant: `outline` } },
    { id: `l1`, component: `Link`, props: { label: `Docs`, href: `https://ui.exponential.at/docs` } },
    { id: `l2`, component: `Link`, props: { href: `javascript:alert(1)` } },
    { id: `l3`, component: `Link`, props: { href: `/relative/path`, external: true } },
    { id: `t1`, component: `Toggle`, props: { label: `Bold`, pressed: true } },
    { id: `t2`, component: `Toggle`, props: { icon: `star`, pressed: { path: `/on` } } },
  ],
}

describe(`actions DOM parity with React`, () => {
  for (const c of examples) {
    const loading = JSON.stringify(c.node).includes(`"loading":true`)
    it(c.name, async () => (loading ? expectPendingParity({ tree: c.node }) : expectDomParity({ tree: c.node })))
  }
  it(`icons, icon-only, disabled, links (allowed, denied, relative external), toggles (bound)`, async () => expectDomParity({ tree: bar, data: { on: true } }))
  it(`a submit Button inside a Form`, async () => expectDomParity({ tree: { id: `f`, component: `Form`, children: [{ id: `s`, component: `Button`, props: { label: `Send`, submit: true } }] } as NestedNode }))
  it(`loading with an icon and a pendingLabel, past the delay`, async () => expectPendingParity({ tree: { id: `b`, component: `Button`, props: { label: `Save`, icon: `star`, loading: true, pendingLabel: `Saving` } } }))
  for (const s of [`hover`, `focus`, `pressed`, `checked`, `open`, `disabled`, `invalid`]) it(`forced state ${s}`, async () => expectDomParity({ tree: bar, data: { on: false }, states: [s] }))
  it(`dark + rtl in the exponential theme`, async () => expectDomParity({ tree: bar, data: { on: true }, theme: `exponential`, mode: `dark`, direction: `rtl` }))
})
