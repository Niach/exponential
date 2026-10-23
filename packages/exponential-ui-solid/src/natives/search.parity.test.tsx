// FEED-128: SearchList DOM parity with React (src/test/parity.ts): every
// catalog example, the kitchen-sink list (a picked value), a typed query
// (count, filtered rows, the empty state), disabled rows, maxRows, a bound
// query, forced states, dark + rtl.

import { describe, it } from "vitest"
import type { NestedNode } from "@exponential_app/ui"
import components from "@exponential_app/ui/fixtures/catalog-components.json"
import { expectParity as expectDomParity, sinkNode } from "./table-chips-search.parity"

type Case = { name: string; node: NestedNode }
const examples = (components as { cases: Case[] }).cases.filter((c) => c.node.component === `SearchList`)
const items = Array.from({ length: 14 }, (_, i) => ({ label: `Issue number ${i}`, value: `v${i}`, identifier: i % 3 ? `EXP-${i}` : undefined, icon: i % 2 ? `star` : undefined, disabled: i === 2 }))

describe(`SearchList DOM parity`, () => {
  for (const c of examples) it(c.name, async () => expectDomParity({ tree: c.node }))
  it(`kitchen-sink SearchList (picked value)`, async () => expectDomParity({ tree: sinkNode(`search-list`) }))
  it(`a typed query: count, filtered rows; no match: the empty state`, async () => {
    await expectDomParity({ tree: { id: `s`, component: `SearchList`, props: { items, query: `exp-1`, value: `v4` } } as NestedNode })
    await expectDomParity({ tree: { id: `s`, component: `SearchList`, props: { items, query: `zzz`, emptyText: `Nothing here` } } as NestedNode })
    await expectDomParity({ tree: { id: `s`, component: `SearchList`, props: { items, query: `zzz`, showCount: false } } as NestedNode })
  })
  it(`maxRows, disabled rows, a bound query + value, host search`, async () => {
    await expectDomParity({ tree: { id: `s`, component: `SearchList`, props: { items, maxRows: 3 } } as NestedNode })
    await expectDomParity({ tree: { id: `s`, component: `SearchList`, props: { items, query: { path: `/q` }, value: { path: `/v` }, maxRows: 2.7 } } as unknown as NestedNode, data: { q: `number 1`, v: `v10` } })
    await expectDomParity({ tree: { id: `s`, component: `SearchList`, props: { items, query: `zzz` }, on: { search: { event: { name: `search` } } } } as unknown as NestedNode })
  })
  for (const states of [[`hover`], [`focus`, `selected`], [`disabled`, `pressed`]]) {
    it(`forced states ${states.join(`+`)}`, async () => expectDomParity({ tree: sinkNode(`search-list`), states }))
  }
  it(`dark + rtl, exponential theme`, async () => {
    await expectDomParity({ tree: sinkNode(`search-list`), theme: `exponential`, mode: `dark`, direction: `rtl` })
  })
})
