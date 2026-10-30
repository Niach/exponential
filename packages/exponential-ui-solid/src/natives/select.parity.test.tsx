// FEED-128: DOM parity of natives/select.tsx with React's (every catalog
// fixture case, forced states, dark + rtl, a placeholder, a Form with the
// hidden native select, invalid, an empty list, host-sourced options).

import { describe, it } from "vitest"
import type { NestedNode } from "@exponential_app/eui"
import { expectParity, fixtureCases } from "./inputs-parity"

const STATES = [`hover`, `focus`, `pressed`, `checked`, `open`, `disabled`, `invalid`]
const OPTIONS = [{ label: `Backlog`, value: `backlog`, icon: `star` }, { label: `Sprint`, value: `sprint` }, { label: `Done`, value: `done`, disabled: true }]
const sel = (props: Record<string, unknown>) => ({ id: `s`, component: `Select`, props: { label: `Board`, name: `board`, options: OPTIONS, ...props } }) as unknown as NestedNode

describe(`select: catalog fixture cases`, () => {
  for (const c of fixtureCases([`Select`])) it(c.name, async () => expectParity({ tree: c.node, theme: `neutral` }))
})

describe(`select: variants`, () => {
  const example = fixtureCases([`Select`])[0].node
  it(`every forced state`, async () => expectParity({ tree: example, theme: `exponential`, states: STATES }))
  it(`dark + rtl (orange)`, async () => expectParity({ tree: example, theme: `orange`, mode: `dark`, direction: `rtl` }))
  for (const variant of [{}, { multiple: true }, { searchable: true }, { multiple: true, searchable: true }]) {
    const tag = JSON.stringify(variant)
    it(`${tag}: placeholder (no value)`, async () => expectParity({ tree: sel({ ...variant, placeholder: `Pick one` }) }))
    it(`${tag}: a value not in the options`, async () => expectParity({ tree: sel({ ...variant, value: `nope` }) }))
    it(`${tag}: no options`, async () => expectParity({ tree: sel({ ...variant, options: [], value: `x` }) }))
    it(`${tag}: bound value`, async () => expectParity({ tree: sel({ ...variant, value: { path: `/v` } }), data: { v: variant && `multiple` in variant ? [`backlog`, `sprint`] : `sprint` } }))
    it(`${tag}: a comma list value`, async () => expectParity({ tree: sel({ ...variant, value: `backlog,sprint` }) }))
    it(`${tag}: inside a Form, invalid after submit`, async () =>
      expectParity(
        {
          tree: { id: `f`, component: `Form`, children: [sel({ ...variant, value: { path: `/v` }, checks: [{ condition: { call: `required`, args: { value: { path: `/v` } } }, message: `Pick a board` }] })] } as unknown as NestedNode,
          data: { v: `` },
          theme: `exponential`,
        },
        (root) => {
          root.querySelector(`form`)!.dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
        }
      ))
    it(`${tag}: inside a Form with a value`, async () => expectParity({ tree: { id: `f`, component: `Form`, children: [sel({ ...variant, value: `sprint` })] } as unknown as NestedNode }))
  }
})
