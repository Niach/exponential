// FEED-128 (the inputs + select lanes' parity cases): `expectDomParity`
// with React 19's `useId` form (`_r_1_`, which src/test/parity.ts's
// GENERATED_ID pattern does not cover) folded into a token the helper
// renames, and an optional interaction replayed on BOTH surfaces before the
// DOMs are compared.

import { expect } from "vitest"
import { act } from "react"
import fixture from "@exponential_app/ui/fixtures/catalog-components.json"
import type { NestedNode } from "@exponential_app/ui"
import { renderReact, renderSolidSurface, serializeDom, type ParityCase } from "../test/parity"

const ID_ATTRS = [`id`, `for`, `aria-controls`, `aria-labelledby`, `aria-describedby`, `aria-activedescendant`]
const REACT_ID = /_r_([0-9a-z]+)_/g

/** React's `_r_1_` → `cl-r1` (`radix-_r_1_` is already renamed). */
function foldReactIds(root: Element) {
  for (const el of [root, ...Array.from(root.querySelectorAll(`*`))])
    for (const a of ID_ATTRS) {
      const v = el.getAttribute(a)
      if (v && !v.startsWith(`radix-`) && REACT_ID.test(v)) el.setAttribute(a, v.replace(REACT_ID, `cl-r$1`))
    }
}

/** An interaction = steps; React runs each in its own `act` (a re-render
 *  between steps, as a person's events get). */
export type Interaction = (root: Element) => void | Promise<void>

/** Both surfaces' canonical DOM after `interact` ran on each. The two
 *  render one after the other (focus moves are per document). */
export async function renderPair(c: ParityCase, ...interact: Interaction[]): Promise<{ react: string; solid: string }> {
  const r = await renderReact(c)
  let react: string
  try {
    for (const step of interact)
      await act(async () => {
        await step(r.root)
      })
    foldReactIds(r.root)
    react = serializeDom(r.root)
  } finally {
    r.cleanup()
  }
  const s = await renderSolidSurface(c)
  try {
    for (const step of interact) {
      await step(s.root)
      await new Promise((res) => setTimeout(res, 0))
    }
    return { react, solid: serializeDom(s.root) }
  } finally {
    s.cleanup()
  }
}

export async function expectParity(c: ParityCase, ...interact: Interaction[]): Promise<void> {
  const { react, solid } = await renderPair(c, ...interact)
  expect(solid).toBe(react)
}

/** The catalog fixture's cases of these components (example × every enum
 *  value × both booleans, as React's fixtures.test.tsx iterates them). */
export function fixtureCases(components: readonly string[]): { name: string; node: NestedNode }[] {
  return (fixture as unknown as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => components.includes(c.node.component))
}
