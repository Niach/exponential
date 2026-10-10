// Round 4 (VAPP-103): the TS reference replays fixtures/round4-contract.json
// (the Rust core: tests/round4_fixtures.rs) and pins the rules behind it.

import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/round4-contract.json" with { type: "json" }
import { CORE_CATALOG_ID } from "./catalog"
import { runAction, submitClosesOverlay, withOwnWrites } from "./dynamic"
import { CORE_FUNCTIONS } from "./expr"
import { reduceNested } from "./reducer"
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

describe(`round4-contract.json`, () => {
  test(`validate: the reducer reports bad style keys and tokens, literal colours, unknown functions and dangling slot columns`, () => {
    for (const c of fixture.validate) expect(reduce(c.input as unknown as NestedNode).issues, c.name).toEqual(c.issues)
    const named = (name: string) => fixture.validate.find((c) => c.name === name)!.issues.map((i) => i.message)
    expect(named(`style: a key outside the Box whitelist`)).toEqual([`style.fontColor: not in the Box style whitelist`])
    expect(named(`style: an unknown token`)[0]).toContain(`style.gap`)
    expect(named(`prop: a colour token passes`)).toEqual([])
    expect(named(`action: a namespaced host function passes`)).toEqual([])
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
