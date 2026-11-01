// EXP-1604: DOM parity of Motion (natives/motion.tsx) with React: every
// catalog-components.json example, a bound progress, a styled box (the
// style wins over the composition size), an inline Lottie, a src (the box
// waits for the file), a decorative one, dark + rtl.

import { describe, it } from "vitest"
import fixture from "@exponential_app/eui/fixtures/catalog-components.json"
import type { NestedNode } from "@exponential_app/eui"
import { expectDomParity } from "../test/parity"

const cases = (fixture as unknown as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => c.node.component === `Motion`)
const dot = { size: [40, 20], duration: 500, layers: [{ type: `shape`, shapes: [{ type: `ellipse`, position: [20, 10], size: [8, 8] }, { type: `fill`, color: `$color.primary` }] }] }

describe(`DOM parity: motion`, { timeout: 60_000 }, () => {
  for (const c of cases) it(c.name, async () => expectDomParity({ tree: c.node }))
  it(`bound progress, styled box, inline lottie, src, decorative`, async () =>
    expectDomParity({
      tree: {
        id: `root`,
        component: `Stack`,
        children: [
          { id: `m1`, component: `Motion`, props: { ...dot, progress: { path: `/p` }, label: `Upload` } },
          { id: `m2`, component: `Motion`, props: { ...dot, fit: `cover` }, style: { width: 120, height: 40 } },
          { id: `m3`, component: `Motion`, props: { ...dot }, style: { width: 64 } },
          { id: `m4`, component: `Motion`, props: { lottie: { v: `5.7.0`, fr: 30, ip: 0, op: 30, w: 30, h: 60, layers: [] }, label: `Lottie` } },
          { id: `m5`, component: `Motion`, props: { src: `/api/attachments/abc`, play: false } },
        ],
      } as NestedNode,
      data: { p: 0.4 },
    }))
  it(`dark rtl`, async () => expectDomParity({ tree: { id: `m`, component: `Motion`, props: { ...dot, label: `Dot` } } as NestedNode, theme: `exponential`, mode: `dark`, direction: `rtl` }))
})
