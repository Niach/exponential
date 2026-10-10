// Round 4 (VAPP-103): the TS reference replays fixtures/round4-contract.json
// (the Rust core: tests/round4_fixtures.rs) and pins the rules behind it.

import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/round4-contract.json" with { type: "json" }
import { CORE_CATALOG_ID } from "./catalog"
import { readPointer, runAction, submitClosesOverlay, withOwnWrites, writePointer } from "./dynamic"
import { CORE_FUNCTIONS } from "./expr"
import { reduceNested } from "./reducer"
import { templateBudget, templateSiteKey } from "./list"
import { isCallableName } from "./validate"
import type { Action, NestedNode, UiNode } from "./types"

const reduce = (input: NestedNode) => reduceNested(input, { catalogId: CORE_CATALOG_ID })
const byId = (root: UiNode, id: string): UiNode | undefined => {
  if (root.id === id) return root
  for (const n of [...Object.values(root.slots ?? {}), ...root.children]) {
    const hit = byId(n, id)
    if (hit) return hit
  }
  return undefined
}

/** `{ "$fill": n }` = n zeros. */
const expandFill = (v: unknown): unknown => {
  if (Array.isArray(v)) return v.map(expandFill)
  if (!v || typeof v !== `object`) return v
  const o = v as Record<string, unknown>
  return typeof o.$fill === `number` ? Array(o.$fill).fill(0) : Object.fromEntries(Object.entries(o).map(([k, x]) => [k, expandFill(x)]))
}

const placeholders = (result: { root: UiNode; templates?: Record<string, UiNode> }) => {
  const out: { tree: string; id: string; component: unknown }[] = []
  const walk = (tree: string, n: UiNode) => {
    if (n.component === `Unknown`) out.push({ tree, id: n.id, component: n.props.component })
    for (const c of n.children) walk(tree, c)
    for (const c of Object.values(n.slots ?? {})) walk(tree, c)
  }
  walk(`root`, result.root)
  for (const [id, n] of Object.entries(result.templates ?? {})) walk(id, n)
  return out
}

describe(`round4-contract.json`, () => {
  test(`validate: the reducer reports bad style keys and tokens, literal colours, unknown functions and dangling slot columns`, () => {
    for (const c of fixture.validate) expect(reduce(c.input as unknown as NestedNode).issues, c.name).toEqual(c.issues)
    const named = (name: string) => fixture.validate.find((c) => c.name === name)!.issues.map((i) => i.message)
    expect(named(`style: a key outside the Box whitelist`)).toEqual([`style.fontColor: not in the Box style whitelist`])
    expect(named(`style: an unknown token`)[0]).toContain(`style.gap`)
    expect(named(`prop: a colour token passes`)).toEqual([])
    expect(named(`action: a namespaced host function passes`)).toEqual([])
  })

  test(`depth: maxDepth holds after expansion, each lifted template from its own level 1`, () => {
    for (const c of fixture.depth) {
      const result = reduce(c.input as unknown as NestedNode)
      expect(result.issues, c.name).toEqual(c.issues)
      expect(placeholders(result), c.name).toEqual(c.placeholders)
    }
  })

  test(`templateBudget: items charge maxTemplateItems and their subtree's nodes depth-first; the first misfit and the rest are not built, ONE issue`, () => {
    for (const c of fixture.templateBudget) {
      const { root, templates } = reduce(c.input as unknown as NestedNode)
      const budget = templateBudget(root, expandFill(c.data), (id) => templates?.[id])
      for (const site of c.sites) expect(budget.allowed.get(templateSiteKey(site.holder, site.scope)) ?? 0, `${c.name}: ${site.holder} ${site.scope}`).toBe(site.built)
      expect(budget.issue ? [budget.issue] : [], c.name).toEqual(c.issues)
    }
  })

  test(`filter: the core function keeps the matching items in order`, () => {
    for (const c of fixture.filter) expect(CORE_FUNCTIONS.filter!(c.args as Record<string, unknown>), c.name).toEqual(c.expected)
  })

  test(`ownWrite: the context resolves after the component's own write`, () => {
    for (const c of fixture.ownWrite) {
      const node = byId(reduce(c.input as unknown as NestedNode).root, c.target)!
      const outcome = runAction(node.on![c.event] as Action, withOwnWrites(c.data, node.props, c.own))
      expect(outcome.data, c.name).toEqual(c.expected.data)
      expect({ name: outcome.event!.name, context: { ...outcome.event!.context, ...c.sent } }, c.name).toEqual(c.expected.action as never)
    }
    // Without the rule the context would read the stale "" (the bug).
    const stale = fixture.ownWrite[0]!
    const node = byId(reduce(stale.input as unknown as NestedNode).root, stale.target)!
    expect(runAction(node.on!.change as Action, stale.data).event!.context).toEqual({ query: `` })
  })

  test(`closeOnSubmit: the nearest Dialog or Drawer around the Form closes`, () => {
    for (const c of fixture.closeOnSubmit) expect(submitClosesOverlay(reduce(c.input as unknown as NestedNode).root, c.form), c.name).toBe(c.expected.overlay)
  })
})

describe(`host functions are namespaced`, () => {
  test(`catalog names and dotted names are callable; anything else is a typo`, () => {
    for (const ok of [`openUrl`, `filter`, `set`, `formatRelativeTime`, `app.toast`, `harness.openIssue`, `cart.add`, `a.b.c`]) expect(isCallableName(ok), ok).toBe(true)
    for (const bad of [`opneUrl`, `toast`, `.toast`, `app.`, `app..toast`, ``]) expect(isCallableName(bad), bad).toBe(false)
  })
})

describe(`pointers: \`/\` is the whole model for reads as for writes`, () => {
  test(`readPointer("/") = the model; only own keys and indices read`, () => {
    const data = { "": 5, a: [1, 2] }
    expect(readPointer(data, `/`)).toBe(data)
    expect(readPointer(data, ``)).toBe(data)
    expect(readPointer(writePointer(data, `/`, { b: 2 }).data, `/`)).toEqual({ b: 2 })
    expect(readPointer({}, `/constructor`)).toBeUndefined()
    expect(readPointer({}, `/__proto__`)).toBeUndefined()
    expect(readPointer(data, `/a/length`)).toBeUndefined()
    expect(readPointer(data, `/a/1`)).toBe(2)
  })
})
