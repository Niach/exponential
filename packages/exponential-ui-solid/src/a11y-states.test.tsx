// #EXP-1629: `accessibility.selected` / `.expanded` on the web (Solid)
// (fixtures/a11y-states.json): aria-selected (+ aria-current on a node with
// no selection role of its own) and aria-expanded; unset = no attribute.

import { readFileSync } from "node:fs"
import { join } from "node:path"
import { render } from "@solidjs/testing-library"
import { describe, expect, it } from "vitest"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"

type Case = { name: string; data: Record<string, unknown>; tree: NestedNode; expect: Record<string, { selected?: boolean; expanded?: boolean }>; unset: string[] }
const fixture = JSON.parse(readFileSync(join(__dirname, `../../exponential-ui/fixtures/a11y-states.json`), `utf8`)) as { cases: Case[]; invalid: { node: NestedNode; path: string }[] }

describe(`accessibility selected + expanded`, () => {
  for (const c of fixture.cases) {
    it(c.name, () => {
      const reduced = reduceNested(c.tree, { catalogId: CORE_CATALOG_ID })
      expect(reduced.issues).toEqual([])
      const { container } = render(() => <ExponentialSurface root={reduced.root!} templates={reduced.templates} data={c.data} theme="neutral" />)
      const node = (id: string) => container.querySelector(`[data-xui-id="${id}"]`)!
      for (const [id, states] of Object.entries(c.expect)) {
        expect(node(id), id).not.toBeNull()
        if (states.selected !== undefined) {
          expect(node(id).getAttribute(`aria-selected`), id).toBe(String(states.selected))
          expect(node(id).getAttribute(`aria-current`), id).toBe(states.selected ? `true` : null)
        }
        if (states.expanded !== undefined) expect(node(id).getAttribute(`aria-expanded`), id).toBe(String(states.expanded))
      }
      for (const id of c.unset) {
        for (const attr of [`aria-selected`, `aria-current`, `aria-expanded`]) expect(node(id).hasAttribute(attr), `${id} ${attr}`).toBe(false)
      }
    })
  }
  it(`a non-boolean state is an issue, the key dropped`, () => {
    for (const { node, path } of fixture.invalid) {
      const reduced = reduceNested(node, { catalogId: CORE_CATALOG_ID })
      expect(reduced.issues.map((i) => i.path)).toEqual([`/components/${node.id}${path}`])
    }
  })
})
