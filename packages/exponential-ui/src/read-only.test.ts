// VAPP-200: fixtures/read-only.json replayed through the TS reference.
import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/read-only.json" with { type: "json" }
import { CORE_CATALOG_ID, reduceNested } from "./index"
import type { NestedNode, UiNode } from "./types"
import { readOnlyDisabledIds, readOnlyTemplates, readOnlyTree } from "./read-only"

describe(`read-only.json`, () => {
  for (const c of fixture.cases as { name: string; node: NestedNode; disabled: string[] }[])
    it(c.name, () => {
      const { root, templates, issues } = reduceNested(c.node, { catalogId: CORE_CATALOG_ID })
      expect(issues).toEqual([])
      const lifted = templates ?? {}
      const ids = [...readOnlyDisabledIds(root!), ...Object.keys(lifted).sort().flatMap((k) => readOnlyDisabledIds(lifted[k]))]
      expect(ids).toEqual(c.disabled)
      // The pass itself: exactly those ids carry `disabled: true` now, and
      // a second pass changes nothing.
      const out = readOnlyTree(root!)
      const outTemplates = readOnlyTemplates(lifted)
      const flagged: string[] = []
      const walk = (n: UiNode, before: UiNode) => {
        if (n.props.disabled === true && before.props.disabled !== true) flagged.push(n.id)
        for (const [k, s] of Object.entries(n.slots ?? {})) walk(s, before.slots![k])
        n.children.forEach((ch, i) => walk(ch, before.children[i]))
      }
      walk(out, root!)
      for (const k of Object.keys(lifted).sort()) walk(outTemplates[k], lifted[k])
      expect(flagged).toEqual(c.disabled)
      expect(readOnlyTree(out)).toBe(out)
      expect(readOnlyTemplates(outTemplates)).toBe(outTemplates)
    })
  it(`a surface without writing controls is returned as is`, () => {
    const { root } = reduceNested({ id: `r`, component: `Stack`, children: [{ id: `t`, component: `Text`, props: { text: `Hi` } }] }, { catalogId: CORE_CATALOG_ID })
    expect(readOnlyTree(root!)).toBe(root!)
  })
})
