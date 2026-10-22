// FEED-128: DOM parity of CodeBlock (natives/code.tsx) with React: every
// catalog-components.json example (each language, lineNumbers, wrap,
// copyable, variant, size), forced states, and the extras (title, highlight,
// maxLines, an empty line, dark + rtl).

import { describe, it } from "vitest"
import fixture from "@exponential-at/ui/fixtures/catalog-components.json"
import type { NestedNode } from "@exponential-at/ui"
import { expectDomParity } from "../test/parity"

const cases = (fixture as unknown as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => c.node.component === `CodeBlock`)

describe(`DOM parity: CodeBlock`, { timeout: 60_000 }, () => {
  for (const c of cases) it(c.name, async () => expectDomParity({ tree: c.node }))
  it(`CodeBlock: forced states`, async () => expectDomParity({ tree: cases[0]!.node, states: [`hover`, `focus`, `pressed`, `checked`, `selected`] }))
  it(`CodeBlock: title, highlight, maxLines, an empty line, plain without a header, dark + rtl`, async () => {
    const tree = {
      id: `root`,
      component: `Stack`,
      children: [
        { id: `a`, component: `CodeBlock`, props: { code: `const x = 1 // hi\n\nreturn x`, language: `ts`, lineNumbers: true, highlight: [1, 3], title: `a.ts`, maxLines: 2, wrap: true } },
        { id: `b`, component: `CodeBlock`, props: { code: `plain text`, copyable: false } },
        { id: `c`, component: `CodeBlock`, props: { code: `\u001b[31mred\u001b[0m done`, language: `ansi`, variant: `destructive`, size: `sm` } },
      ],
    } as unknown as NestedNode
    await expectDomParity({ tree })
    await expectDomParity({ tree, theme: `exponential`, mode: `dark`, direction: `rtl`, locale: `de-AT` })
  })
})
