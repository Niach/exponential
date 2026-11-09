// #EDE-65 lane M: DOM parity of Embed (natives/embed.tsx) with React: every
// catalog-components.json example, every fixtures/embed.json `render` case
// (in place, own data, bound, template rows, the error box, nested), dark
// + rtl.

import { describe, it } from "vitest"
import components from "@exponential_app/eui/fixtures/catalog-components.json"
import embed from "@exponential_app/eui/fixtures/embed.json"
import type { NestedNode } from "@exponential_app/eui"
import { expectDomParity } from "../test/parity"

const examples = (components as unknown as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => c.node.component === `Embed`)
const renders = (embed as unknown as { render: { name: string; root: NestedNode; data: Record<string, unknown> }[] }).render

describe(`DOM parity: embed`, { timeout: 60_000 }, () => {
  for (const c of examples) it(c.name, async () => expectDomParity({ tree: c.node }))
  for (const c of renders) it(`embed.json ${c.name}`, async () => expectDomParity({ tree: c.root, data: c.data }))
  it(`dark rtl, an authored alignSelf`, async () =>
    expectDomParity({
      tree: { id: `e`, component: `Embed`, props: { surface: { root: { id: `t`, component: `Text`, props: { text: `Hi` } } } }, style: { alignSelf: `center` } } as NestedNode,
      theme: `exponential`,
      mode: `dark`,
      direction: `rtl`,
    }))
})
