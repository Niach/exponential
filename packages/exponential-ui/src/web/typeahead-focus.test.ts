import { describe, expect, it } from "bun:test"
import fixture from "../../fixtures/interactions-round1.json"
import { isFieldAnchor } from "./anchor"

interface Node {
  id: string
  component: string
  props?: Record<string, unknown>
  children?: Node[]
}

/** The node `id` in `tree` (a `<node>.field` id = that node's field part). */
function find(tree: Node, id: string): { component: string; part: string | null } | null {
  const [base, part] = id.split(`.`)
  const walk = (n: Node): Node | null => (n.id === base ? n : (n.children ?? []).reduce<Node | null>((hit, c) => hit ?? walk(c), null))
  const hit = walk(tree)
  return hit ? { component: hit.component, part: part ?? null } : null
}

describe(`interactions-round1.json › typeaheadFocus (#EDE-65)`, () => {
  const fx = fixture.typeaheadFocus as unknown as { surfaces: Record<string, Node>; cases: { name: string; surface: string; typeahead: boolean }[] }
  for (const c of fx.cases) {
    it(c.name, () => {
      const tree = fx.surfaces[c.surface]!
      const menu = (tree.children ?? []).find((n) => n.id === `menu`)!
      const anchor = typeof menu.props?.anchor === `string` ? find(tree, menu.props.anchor) : null
      expect(anchor ? isFieldAnchor(anchor.component, anchor.part) : false).toBe(c.typeahead)
    })
  }
  it(`a field part anchors a typeahead too; another part does not`, () => {
    expect(isFieldAnchor(`Input`, `field`)).toBe(true)
    expect(isFieldAnchor(`Textarea`, null)).toBe(true)
    expect(isFieldAnchor(`Input`, `label`)).toBe(false)
    expect(isFieldAnchor(`Select`, `search`)).toBe(false)
  })
})
