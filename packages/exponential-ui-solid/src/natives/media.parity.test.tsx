// FEED-128: DOM parity of the media natives (natives/media.tsx) with React:
// every catalog-components.json example of Image / Video / AudioPlayer /
// Avatar / Carousel, forced states, and the extras (focal point, size,
// fallback icon, poster, autoplay, an Avatar with a src and a seed, a
// many-page Carousel, dark + rtl).

import { describe, expect, it } from "vitest"
import { act } from "react"
import fixture from "@exponential-at/ui/fixtures/catalog-components.json"
import type { NestedNode } from "@exponential-at/ui"
import { expectDomParity, renderReact, renderSolidSurface, serializeDom, type ParityCase } from "../test/parity"

const tick = () => new Promise((r) => setTimeout(r, 0))

/** Past the first timers (Radix Avatar's fallback `delayMs={0}` is a 0 ms
 *  timeout, React's act does not always flush it). */
async function expectSettledParity(c: ParityCase): Promise<void> {
  const r = await renderReact(c)
  let react: string
  try {
    await act(async () => {
      await tick()
    })
    react = serializeDom(r.root)
  } finally {
    r.cleanup()
  }
  const s = await renderSolidSurface(c)
  try {
    await tick()
    expect(serializeDom(s.root)).toBe(react)
  } finally {
    s.cleanup()
  }
}

const COMPONENTS = [`Image`, `Video`, `AudioPlayer`, `Avatar`, `Carousel`]
const cases = (fixture as unknown as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => COMPONENTS.includes(c.node.component))

describe(`DOM parity: media`, { timeout: 60_000 }, () => {
  for (const c of cases) it(c.name, async () => expectDomParity({ tree: c.node }))
  for (const component of COMPONENTS) {
    const c = cases.find((x) => x.node.component === component)!
    it(`${component}: forced states`, async () => expectDomParity({ tree: c.node, states: [`hover`, `focus`, `pressed`, `selected`, `disabled`] }))
  }
  const extras = {
    id: `root`,
    component: `Stack`,
    children: [
      { id: `i1`, component: `Image`, props: { src: `https://example.com/a.png`, alt: `A`, focalX: 0.2, focalY: 1, width: 120, height: 80, fit: `scaleDown` } },
      { id: `i2`, component: `Image`, props: { alt: `Empty`, fallback: `star` } },
      { id: `i3`, component: `Image`, props: { src: `javascript:alert(1)`, alt: `Denied` } },
      { id: `v1`, component: `Video`, props: { src: `https://example.com/c.mp4`, poster: `https://example.com/p.jpg`, autoplay: true, aspectRatio: 1 } },
      { id: `a1`, component: `AudioPlayer`, props: { src: `https://example.com/t.mp3`, durationMs: 3000 } },
      { id: `av1`, component: `Avatar`, props: { name: `Jane Doe`, src: `https://example.com/j.png`, size: `lg` } },
      { id: `av2`, component: `Avatar`, props: { name: ``, seed: `x` } },
      { id: `av3`, component: `Avatar`, props: {} },
      {
        id: `c1`,
        component: `Carousel`,
        props: { page: 1, loop: true },
        children: [
          { id: `p1`, component: `Text`, props: { text: `one` } },
          { id: `p2`, component: `Text`, props: { text: `two` } },
          { id: `p3`, component: `Text`, props: { text: `three` } },
        ],
      },
      { id: `c2`, component: `Carousel`, props: { page: 0 }, children: [{ id: `q1`, component: `Text`, props: { text: `one` } }, { id: `q2`, component: `Text`, props: { text: `two` } }] },
    ],
  } as unknown as NestedNode
  it(`extras: focal point, size, fallback icon, a denied src, poster + autoplay, Avatar src/seed/empty, many pages`, async () => expectSettledParity({ tree: extras }))
  it(`extras in dark + rtl + de-AT`, async () => expectSettledParity({ tree: extras, theme: `exponential`, mode: `dark`, direction: `rtl`, locale: `de-AT` }))
})
