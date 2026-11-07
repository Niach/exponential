// #VAPP-277: Sidebar `rail` — the `$pass` that hands the Sidebar's
// `collapsed` to the Rows and Sections of its `sidebar` slot before they
// expand, the conditional slot splices, the logical tooltip side.
// fixtures/catalog-macros.json (Sidebar/rail:*) and bind-time.json
// (Sidebar/rail:bound-collapsed) lock the trees; this pins the rules.

import { describe, expect, test } from "bun:test"
import { CORE_CATALOG_ID } from "./catalog"
import { reduceNested } from "./reducer"
import type { NestedNode, UiNode } from "./types"
import { tooltipPhysicalSide } from "./web/focus"

const find = (node: UiNode, id: string): UiNode | undefined => {
  if (node.id === id) return node
  for (const c of [...Object.values(node.slots ?? {}), ...node.children]) {
    const hit = find(c, id)
    if (hit) return hit
  }
  return undefined
}

const row = (id: string, extra: Record<string, unknown> = {}, slots?: Record<string, NestedNode>): NestedNode => ({ id, component: `Row`, props: { title: id, icon: `nav-inbox`, ...extra }, ...(slots ? { slots } : {}) })
const shell = (props: Record<string, unknown>, sidebar: NestedNode): NestedNode => ({ id: `shell`, component: `Sidebar`, props: { collapseBelow: `never`, ...props }, slots: { sidebar }, children: [] })
const reduce = (tree: NestedNode) => {
  const { root, issues } = reduceNested(tree, { catalogId: CORE_CATALOG_ID })
  expect(issues).toEqual([])
  return root
}

describe(`Sidebar rail (#VAPP-277)`, () => {
  test(`collapsed + rail: the column stays, its Rows (nested ones too) become rail tiles`, () => {
    const root = reduce(shell({ rail: true, collapsed: true }, { id: `nav`, component: `Stack`, children: [row(`a`, { meta: `3` }), { id: `sec`, component: `Section`, props: { title: `More` }, children: [row(`b`)] }] }))
    expect(find(root, `shell.sidebar`)?.style?.width).toBe(`$spacing.xl3`)
    for (const id of [`a`, `b`]) {
      expect(find(root, `${id}.body`)).toBeUndefined()
      expect(find(root, `${id}.tip`)?.props).toEqual({ content: id, side: `end` })
    }
    expect(find(root, `a`)?.accessibility?.label).toBe(`a, 3`)
    expect(find(root, `a.tip.tile.badge.count`)?.props.text).toBe(`3`)
    expect(find(root, `b.tip.tile.badge`)).toBeUndefined()
  })

  test(`without rail, collapsed hides the column as before; rail + expanded = the plain Rows and Sections`, () => {
    expect(find(reduce(shell({ collapsed: true }, row(`a`))), `shell.sidebar`)).toBeUndefined()
    const nav = (): NestedNode => ({ id: `nav`, component: `Stack`, children: [row(`a`, {}, { trailing: { id: `t`, component: `Badge`, props: { text: `New` } } }), { id: `sec`, component: `Section`, props: { title: `More` }, children: [row(`b`)] }] })
    const plain = reduce(shell({}, nav()))
    const railExpanded = reduce(shell({ rail: true, collapsed: false }, nav()))
    expect(find(railExpanded, `nav`)).toEqual(find(plain, `nav`))
    expect(find(railExpanded, `shell.sidebar`)?.style?.width).toBe(240)
  })

  test(`the pass stops at a nested Sidebar and never reaches the main children`, () => {
    const inner = shell({ collapsed: false }, row(`inner`))
    const tree: NestedNode = { ...shell({ rail: true, collapsed: true }, { id: `nav`, component: `Stack`, children: [row(`a`), { ...inner, id: `inner-shell` }] }), children: [row(`main`)] }
    const root = reduce(tree)
    expect(find(root, `a.tip`)).toBeDefined()
    expect(find(root, `inner.tip`)).toBeUndefined()
    expect(find(root, `main.tip`)).toBeUndefined()
  })

  test(`a rail Row drops its leading and trailing slots (bound: visible = not collapsed, and-ed with the slot's own)`, () => {
    const slotted = (collapsed: unknown) =>
      reduce(shell({ rail: true, collapsed }, { id: `nav`, component: `Stack`, children: [row(`a`, {}, { leading: { id: `lead`, component: `Icon`, props: { name: `nav-issues` } }, trailing: { id: `trail`, component: `Badge`, props: { text: `New` }, visible: { "@path": `/show` } } })] }))
    const rail = slotted(true)
    expect(find(rail, `lead`)).toBeUndefined()
    expect(find(rail, `trail`)).toBeUndefined()
    const bound = slotted({ "@path": `/c` })
    expect(find(bound, `lead`)?.visible).toEqual({ "@call": `not`, args: { value: { "@call": `cond`, args: { if: { "@path": `/c` }, then: true, else: false } } } })
    expect(find(bound, `trail`)?.visible).toEqual({ "@call": `and`, args: { values: [{ "@path": `/show` }, { "@call": `not`, args: { value: { "@call": `cond`, args: { if: { "@path": `/c` }, then: true, else: false } } } }] } })
  })

  test(`a Section on the rail: no header, a rule instead, its rows shown even when closed, undivided`, () => {
    const root = reduce(shell({ rail: true, collapsed: true }, { id: `sec`, component: `Section`, props: { title: `More`, collapsible: true, open: false }, children: [row(`b`)] }))
    expect(find(root, `sec.header`)).toBeUndefined()
    expect(find(root, `sec.rule`)?.accessibility).toEqual({ role: `separator` })
    expect(find(root, `sec.body`)?.props.divided).toBe(false)
    expect(find(root, `b.tip`)).toBeDefined()
  })

  test(`the tooltip points into the content: end for a left Sidebar, start for a right one; RTL mirrors both`, () => {
    const right = reduce(shell({ rail: true, collapsed: true, side: `right` }, row(`a`)))
    expect(find(right, `a.tip`)?.props.side).toBe(`start`)
    expect([tooltipPhysicalSide(`end`, `ltr`), tooltipPhysicalSide(`end`, `rtl`), tooltipPhysicalSide(`start`, `ltr`), tooltipPhysicalSide(`start`, `rtl`), tooltipPhysicalSide(`bottom`, `rtl`)]).toEqual([`right`, `left`, `left`, `right`, `bottom`])
  })
})
