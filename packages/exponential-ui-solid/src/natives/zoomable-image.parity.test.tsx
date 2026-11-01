// EXP-1586: DOM parity of the ZoomableImage native with React: every
// catalog-components.json case, the extras (no pill, a styled height, a tall
// picture, a bound scale, an unknown size), forced states, dark + rtl, and a
// MEASURED box (the controller painted the transform, the cursor and the
// touch-action on both sides).

import { afterEach, beforeEach, describe, it } from "vitest"
import fixture from "@exponential_app/eui/fixtures/catalog-components.json"
import type { NestedNode } from "@exponential_app/eui"
import { expectDomParity } from "../test/parity"

const cases = (fixture as unknown as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => c.node.component === `ZoomableImage`)

const extras = {
  id: `root`,
  component: `Stack`,
  children: [
    { id: `z1`, component: `ZoomableImage`, props: { src: `https://example.com/a.png`, alt: `A`, naturalWidth: 1440, naturalHeight: 900, controls: `none` } },
    { id: `z2`, component: `ZoomableImage`, props: { src: `https://example.com/b.png`, alt: `B`, naturalWidth: 800, naturalHeight: 600 }, style: { height: 240 } },
    { id: `z3`, component: `ZoomableImage`, props: { src: `https://example.com/c.png`, alt: `Tall`, naturalWidth: 390, naturalHeight: 3000, controls: `always` } },
    { id: `z4`, component: `ZoomableImage`, props: { src: `https://example.com/d.png`, alt: `D`, naturalWidth: 1000, naturalHeight: 1000, scale: 2, maxScale: 4, controls: `always` } },
    { id: `z5`, component: `ZoomableImage`, props: { src: `https://example.com/e.png`, alt: `` } },
  ],
} as unknown as NestedNode

describe(`DOM parity: ZoomableImage`, { timeout: 60_000 }, () => {
  for (const c of cases) it(c.name, async () => expectDomParity({ tree: c.node }))
  it(`extras: no pill, a styled height, tall, a bound scale, an unknown size`, async () => expectDomParity({ tree: extras }))
  it(`extras in dark + rtl + de-AT`, async () => expectDomParity({ tree: extras, theme: `exponential`, mode: `dark`, direction: `rtl`, locale: `de-AT` }))
  it(`forced states`, async () => expectDomParity({ tree: extras, states: [`hover`, `focus`, `disabled`, `selected`] }))
})

describe(`DOM parity: ZoomableImage, measured`, { timeout: 60_000 }, () => {
  const width = Object.getOwnPropertyDescriptor(Element.prototype, `clientWidth`)!
  const height = Object.getOwnPropertyDescriptor(Element.prototype, `clientHeight`)!
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, `clientWidth`, { configurable: true, get: () => 600 })
    Object.defineProperty(HTMLElement.prototype, `clientHeight`, { configurable: true, get: () => 400 })
  })
  afterEach(() => {
    delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth
    delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight
    void width
    void height
  })
  it(`the rest views, the bound scale and the pill's ends`, async () => expectDomParity({ tree: extras }))
})
