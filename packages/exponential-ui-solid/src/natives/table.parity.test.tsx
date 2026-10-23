// FEED-128: Table DOM parity with React (src/test/parity.ts): every catalog
// example, the bound/templated slot cells of bind-time.json `rowSlots`, the
// kitchen-sink Table, cell types, selection states, the windowed body, the
// empty state, forced states, dark + rtl.

import { describe, it } from "vitest"
import type { NestedNode } from "@exponential_app/ui"
import components from "@exponential_app/ui/fixtures/catalog-components.json"
import bindTime from "@exponential_app/ui/fixtures/bind-time.json"
import { expectParity as expectDomParity, sinkNode } from "./table-chips-search.parity"

type Case = { name: string; node: NestedNode }
const examples = (components as { cases: Case[] }).cases.filter((c) => c.node.component === `Table`)

const cols = [
  { key: `name`, label: `Name`, sortable: true },
  { key: `n`, label: `N`, type: `number`, decimals: 1, align: `end` },
  { key: `c`, label: `C`, type: `currency`, currency: `EUR` },
  { key: `bad`, label: `Bad`, type: `currency`, currency: `EURO` },
  { key: `p`, label: `P`, type: `percent` },
  { key: `d`, label: `D`, type: `date` },
  { key: `ok`, label: `Ok`, type: `boolean` },
  { key: `b`, label: `B`, type: `badge`, width: 90 },
]
const typedRows = [
  { id: `x`, name: `Xeno`, n: 3.14159, c: 12.5, bad: 1, p: 0.25, d: `2026-01-02T10:00:00Z`, ok: true, b: `Owner` },
  { id: `y`, name: `Abe`, n: `7`, c: `x`, bad: 2, p: null, d: ``, ok: false, b: `` },
  { id: `y`, name: ``, n: null, c: 1, p: 1, ok: 0 },
]

describe(`Table DOM parity`, () => {
  for (const c of examples) it(c.name, async () => expectDomParity({ tree: c.node }))
  for (const e of (bindTime as { extra: { name: string; input: NestedNode; datasets: { data: Record<string, unknown> }[] }[] }).extra.filter((x) => x.name.startsWith(`Table/`))) {
    e.datasets.forEach((d, i) => it(`${e.name} dataset ${i}`, async () => expectDomParity({ tree: e.input, data: d.data })))
  }
  it(`kitchen-sink Table (multiple, bound selected, sort, caption, striped, slot)`, async () => {
    await expectDomParity({ tree: sinkNode(`table`), data: { ui: { selectedMembers: [`bob`] } } })
    await expectDomParity({ tree: sinkNode(`table`), data: { ui: { selectedMembers: [`bob`, `alice`] } } })
    await expectDomParity({ tree: sinkNode(`table`), data: {} })
  })
  it(`cell types, duplicate keys, a local sort desc, single selection`, async () => {
    await expectDomParity({ tree: { id: `t`, component: `Table`, props: { columns: cols, rows: typedRows, sort: { key: `name`, direction: `desc` }, selectable: `single`, selected: [`x`] } } as NestedNode })
  })
  it(`rowPress without selection, emptyText, empty rows`, async () => {
    await expectDomParity({ tree: { id: `t`, component: `Table`, props: { columns: cols, rows: [], emptyText: `Nothing` }, on: { rowPress: { event: { name: `p` } } } } as unknown as NestedNode })
    await expectDomParity({ tree: { id: `t`, component: `Table`, props: { columns: cols, rows: [] }, accessibility: { label: `Members` } } as unknown as NestedNode })
  })
  it(`a windowed body (past the threshold), multiple`, async () => {
    const rows = Array.from({ length: 80 }, (_, i) => ({ id: `r${i}`, name: `Row ${i}`, n: i }))
    await expectDomParity({ tree: { id: `t`, component: `Table`, props: { columns: cols.slice(0, 2), rows, selectable: `multiple`, selected: [`r1`] } } as NestedNode })
  })
  it(`a Table inside a Form (the checkbox bubble inputs)`, async () => {
    await expectDomParity({ tree: { id: `f`, component: `Form`, children: [{ id: `t`, component: `Table`, props: { columns: cols.slice(0, 2), rows: typedRows.slice(0, 2), selectable: `multiple`, selected: [`x`] } }] } as unknown as NestedNode })
  })
  for (const states of [[`hover`], [`focus`, `selected`], [`pressed`, `disabled`, `checked`]]) {
    it(`forced states ${states.join(`+`)}`, async () => {
      await expectDomParity({ tree: sinkNode(`table`), data: { ui: { selectedMembers: [`bob`] } }, states })
    })
  }
  it(`dark + rtl, exponential theme`, async () => {
    await expectDomParity({ tree: sinkNode(`table`), data: { ui: { selectedMembers: [`alice`] } }, theme: `exponential`, mode: `dark`, direction: `rtl` })
    await expectDomParity({ tree: examples[0].node, theme: `orange`, mode: `dark`, direction: `rtl`, locale: `de-AT` })
  })
})
