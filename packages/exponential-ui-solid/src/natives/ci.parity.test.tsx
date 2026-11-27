// #EXP-2204: DOM parity of LogView/LogLines and JobGraph (natives/ci.tsx)
// with React: every catalog-components.json case of both, the specimens
// (a failed job with timestamps + a search, a following live log; stages
// with a matrix, needs only), forced states, the list mode of a compact
// surface, a bound selection, dark + rtl.

import { describe, it } from "vitest"
import fixture from "@exponential_app/eui/fixtures/catalog-components.json"
import specimens from "@exponential_app/eui/fixtures/specimens.json"
import type { NestedNode } from "@exponential_app/eui"
import { expectDomParity } from "../test/parity"

const cases = (fixture as unknown as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => c.node.component === `LogView` || c.node.component === `JobGraph`)
const specimen = (id: string) => (specimens as unknown as { specimens: { id: string; node: NestedNode }[] }).specimens.find((s) => s.id === id)!.node

const lines = Array.from({ length: 120 }, (_, i) => (i % 40 === 0 ? `##[group]Step ${i / 40}` : i % 40 === 39 ? `##[endgroup]` : `\u001b[32mok\u001b[0m line ${i} store`))

describe(`DOM parity: LogView + JobGraph`, { timeout: 60_000 }, () => {
  for (const c of cases) it(c.name, async () => expectDomParity({ tree: c.node }))
  it(`LogView specimen`, async () => expectDomParity({ tree: specimen(`exponential-ui-log-view`), theme: `exponential`, mode: `dark` }))
  it(`JobGraph specimen (graph + the list on a phone)`, async () => {
    await expectDomParity({ tree: specimen(`exponential-ui-job-graph`), theme: `exponential`, mode: `dark`, width: 1280 })
    await expectDomParity({ tree: specimen(`exponential-ui-job-graph`), width: 390 })
  })
  it(`forced states`, async () => {
    await expectDomParity({ tree: cases[0]!.node, states: [`hover`, `focus`, `pressed`, `selected`, `disabled`] })
    await expectDomParity({ tree: cases.find((c) => c.node.component === `JobGraph`)!.node, states: [`hover`, `highlight`, `selected`] })
  })
  it(`a long log: the window, a bound selection (revealed), bound query + open, dark + rtl`, async () => {
    const tree = { id: `log`, component: `LogView`, props: { lines, selection: { "@path": `/sel` }, query: { "@path": `/q` }, open: { "@path": `/open` }, follow: { "@path": `/follow` }, maxLines: 10 } } as unknown as NestedNode
    const data = { sel: `L60-L62`, q: `store`, open: [0], follow: true }
    await expectDomParity({ tree, data })
    await expectDomParity({ tree, data, theme: `exponential`, mode: `dark`, direction: `rtl`, locale: `de-AT` })
  })
})
