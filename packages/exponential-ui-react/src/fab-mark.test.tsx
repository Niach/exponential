// #EXP-1876: Button `variant: fab` + `size: lg` = THE mark button: its icon
// part carries the sheen (`backgroundClip: text` + `backgroundGradient`), so
// the glyph paints as a mask over the gradient; the plain fab stays flat.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"

afterEach(cleanup)

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root

describe(`Button fab lg`, () => {
  it(`paints the mark with the theme's sheen; the plain fab keeps a flat glyph`, () => {
    const root = tree({
      id: `root`,
      component: `Box`,
      children: [
        { id: `mark`, component: `Button`, props: { label: `Assistant`, icon: `brand-exponential`, variant: `fab`, size: `lg` } },
        { id: `fab`, component: `Button`, props: { label: `New issue`, icon: `ui-add`, variant: `fab` } },
      ],
    } as unknown as NestedNode)
    for (const mode of [`dark`, `light`] as const) {
      const { container, unmount } = render(<ExponentialSurface root={root} theme="neutral" mode={mode} id={`m-${mode}`} />)
      const mark = container.querySelector(`[data-xui-id="mark"]`)!
      const glyph = mark.querySelector(`[data-glyph-gradient]`)!
      expect(glyph.getAttribute(`data-icon`)).toBe(`brand-exponential`)
      const stops = [...glyph.querySelectorAll(`stop`)].map((s) => s.getAttribute(`stop-color`))
      expect(stops).toEqual(mode === `dark` ? [`#ffffff`, `#c4c4cc`] : [`#52525b`, `#0a0a0a`])
      expect(glyph.querySelector(`linearGradient`)?.getAttribute(`y2`)).toBe(`1`)
      expect(glyph.querySelector(`mask [data-icon="brand-exponential"]`)).not.toBeNull()
      expect(mark.getAttribute(`aria-label`)).toBe(`Assistant`)
      expect(container.querySelector(`[data-xui-id="fab"] [data-glyph-gradient]`)).toBeNull()
      unmount()
    }
  })
})
