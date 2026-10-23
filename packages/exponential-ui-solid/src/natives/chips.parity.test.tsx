// FEED-128: ChipInput + FileUpload DOM parity with React (src/test/parity.ts):
// every catalog example, the kitchen-sink fields (in their Form), bound
// values, suggestions, max, files with sizes and URLs (the URL policy),
// forced states, dark + rtl.

import { describe, it } from "vitest"
import type { NestedNode } from "@exponential_app/ui"
import components from "@exponential_app/ui/fixtures/catalog-components.json"
import { expectParity as expectDomParity, sinkNode } from "./table-chips-search.parity"

type Case = { name: string; node: NestedNode }
const examples = (components as { cases: Case[] }).cases.filter((c) => c.node.component === `ChipInput` || c.node.component === `FileUpload`)
const form = (...children: NestedNode[]): NestedNode => ({ id: `new-form`, component: `Form`, children }) as unknown as NestedNode
const sink = () => form(sinkNode(`nf-labels`), sinkNode(`nf-files`))
const files = [
  { name: `screenshot.png`, size: 48213, type: `image/png`, url: `https://example.com/s.png` },
  { name: `evil.txt`, size: 0, url: `javascript:alert(1)` },
  { name: `big.pdf`, size: 10485760 * 3 },
  { name: `nosize.bin` },
]

describe(`ChipInput + FileUpload DOM parity`, () => {
  for (const c of examples) it(c.name, async () => expectDomParity({ tree: c.node }))
  it(`kitchen-sink ChipInput + FileUpload in their Form (bound values)`, async () => {
    await expectDomParity({ tree: sink(), data: { form: { labels: [`bug`, `design`] } } })
    await expectDomParity({ tree: sink(), data: {} })
  })
  it(`ChipInput: placeholder when empty, max reached, checks, no label`, async () => {
    await expectDomParity({ tree: { id: `c`, component: `ChipInput`, props: { placeholder: `Add a label`, values: [] } } as NestedNode })
    await expectDomParity({ tree: { id: `c`, component: `ChipInput`, props: { label: `L`, values: [`a`, `b`], max: 2, placeholder: `x` } } as NestedNode })
    await expectDomParity({ tree: form({ id: `c`, component: `ChipInput`, props: { label: `L`, name: `l`, values: [], checks: [{ call: `required`, args: { value: { path: `/l` } }, message: `Required` }], validateOn: `change` } } as unknown as NestedNode) })
  })
  it(`FileUpload: files (sizes, URLs, the URL policy), hint, single, disabled with files`, async () => {
    await expectDomParity({ tree: { id: `f`, component: `FileUpload`, props: { label: `Files`, files, hint: `Up to 10 MB`, multiple: true } } as NestedNode })
    await expectDomParity({ tree: { id: `f`, component: `FileUpload`, props: { files, disabled: true } } as NestedNode, locale: `de-AT` })
    await expectDomParity({ tree: { id: `f`, component: `FileUpload`, props: { files: { path: `/files` } } } as unknown as NestedNode, data: { files: files.slice(0, 1) } })
  })
  for (const states of [[`hover`], [`focus`, `dragover`], [`disabled`, `invalid`, `pressed`]]) {
    it(`forced states ${states.join(`+`)}`, async () => {
      await expectDomParity({ tree: sink(), data: { form: { labels: [`ios`] } }, states })
    })
  }
  it(`dark + rtl, exponential theme`, async () => {
    await expectDomParity({ tree: sink(), data: { form: { labels: [`ios`] } }, theme: `exponential`, mode: `dark`, direction: `rtl` })
  })
})
