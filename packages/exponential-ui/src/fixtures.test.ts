// VAPP-85: the fixtures ARE the contract — the TS reference reducer replays
// every case byte for byte, and the Rust core (VAPP-86) replays the same
// files with these test names.

import { describe, expect, test } from "bun:test"
import { A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID, UNKNOWN_COMPONENT, componentNames, coreCatalog } from "./catalog"
import { expandMacros } from "./macros"
import { preorder, reduceNested, reduceSurface } from "./reducer"
import { validateProps } from "./validate"
import { defineExtension } from "./extension"
import componentsFixture from "../fixtures/catalog-components.json" with { type: "json" }
import macrosFixture from "../fixtures/catalog-macros.json" with { type: "json" }
import basicFixture from "../fixtures/catalog-basic-map.json" with { type: "json" }
import extensionFixture from "../fixtures/catalog-extension.json" with { type: "json" }
import kitchenSink from "../fixtures/kitchen-sink.json" with { type: "json" }
import kitchenExpanded from "../fixtures/kitchen-sink.expanded.json" with { type: "json" }
import type { ExtensionDef, FlatComponent, NestedNode, UiNode } from "./types"

const canon = (value: unknown) => JSON.stringify(value)

function walk(node: UiNode, visit: (n: UiNode) => void): void {
  visit(node)
  for (const slot of Object.values(node.slots ?? {})) walk(slot, visit)
  node.children.forEach((c) => walk(c, visit))
}

describe(`catalog-components.json`, () => {
  const cases = componentsFixture.cases as { name: string; node: NestedNode }[]

  test(`covers every visible component × every enum value × both booleans`, () => {
    const seen = new Set(cases.map((c) => c.node.component))
    expect([...seen].sort()).toEqual(componentNames().sort())
    for (const [name, def] of Object.entries(coreCatalog.components)) {
      if (def.hidden) continue
      for (const [prop, schema] of Object.entries(def.props)) {
        if (schema.type === `enum`) {
          const values = schema.values ?? coreCatalog.enums[schema.enum!]
          for (const value of values) expect(cases.some((c) => c.name === `${name}/${prop}=${value}`), `${name}/${prop}=${value}`).toBe(true)
        }
        if (schema.type === `boolean`) for (const value of [true, false]) expect(cases.some((c) => c.name === `${name}/${prop}=${value}`), `${name}/${prop}=${value}`).toBe(true)
      }
    }
  })

  test(`every case validates and reduces without issues`, () => {
    for (const c of cases) {
      const def = coreCatalog.components[c.node.component]
      expect(validateProps(def, c.node.props ?? {}), c.name).toEqual([])
      const result = reduceNested(c.node, { catalogId: CORE_CATALOG_ID })
      expect(result.issues, c.name).toEqual([])
      walk(result.root, (n) => expect(coreCatalog.components[n.component]?.kind, `${c.name}: ${n.id}`).toBe(`native`))
    }
  })
})

describe(`catalog-macros.json`, () => {
  const cases = macrosFixture.cases as { name: string; input: NestedNode; expected: UiNode }[]

  test(`every macro case expands byte for byte`, () => {
    expect(cases.length).toBeGreaterThan(0)
    for (const c of cases) {
      const { root, issues } = reduceNested(c.input, { catalogId: CORE_CATALOG_ID })
      expect(issues, c.name).toEqual([])
      expect(canon(root), c.name).toBe(canon(c.expected))
    }
  })

  test(`expansion is pure and reaches natives only, ids stay unique, recipes name the macro`, () => {
    for (const c of cases) {
      const before = canon(c.input)
      const first = reduceNested(c.input, { catalogId: CORE_CATALOG_ID, expand: false }).root
      const expanded = expandMacros(first)
      expect(canon(c.input), c.name).toBe(before)
      expect(canon(expandMacros(expanded)), `${c.name} idempotent`).toBe(canon(expanded))
      const ids = preorder(expanded)
      expect(new Set(ids).size, c.name).toBe(ids.length)
      expect(expanded.id).toBe(c.input.id)
      expect(expanded.recipe?.macro).toBe(c.input.component)
      expect(expanded.recipe?.part).toBe(`root`)
    }
  })

  test(`every macro's events route to a part or stay on the root`, () => {
    const emptyState = cases.find((c) => c.name === `EmptyState/example`)!
    const withHandler = { ...emptyState.input, props: { ...emptyState.input.props, actionLabel: `Retry` }, on: { press: { event: { name: `retry` } } } }
    const root = reduceNested(withHandler, { catalogId: CORE_CATALOG_ID }).root
    expect(root.on).toBeUndefined()
    const action = root.children.find((n) => n.id === `empty-state.action`)!
    expect(action.on?.press).toEqual({ event: { name: `retry` } })

    const pagination = cases.find((c) => c.name === `Pagination/example`)!
    const paged = reduceNested({ ...pagination.input, on: { change: { event: { name: `page`, context: { list: `issues` } } } } }, { catalogId: CORE_CATALOG_ID }).root
    expect(paged.children[0].on?.press).toEqual({ event: { name: `page`, context: { list: `issues`, page: 1 } } })
    expect(paged.children[2].on?.press).toEqual({ event: { name: `page`, context: { list: `issues`, page: 3 } } })

    const pill = cases.find((c) => c.name === `Pill/pressable=true`)!
    const pressed = reduceNested({ ...pill.input, on: { press: { event: { name: `pick` } } } }, { catalogId: CORE_CATALOG_ID }).root
    expect(pressed.on?.press).toEqual({ event: { name: `pick` } })
  })
})

describe(`catalog-basic-map.json`, () => {
  const cases = basicFixture.cases as { name: string; components: FlatComponent[]; expected: { root: UiNode; issues: unknown[] } }[]

  test(`every basic case reduces byte for byte`, () => {
    expect(cases.length).toBeGreaterThan(10)
    for (const c of cases) {
      const result = reduceSurface(c.components, { catalogId: A2UI_BASIC_CATALOG_ID })
      expect(canon(result), c.name).toBe(canon(c.expected))
    }
  })

  test(`the unmapped component is the placeholder, never an error`, () => {
    const c = cases.find((x) => x.name.startsWith(`An unmapped component`))!
    const placeholder = c.expected.root.children.find((n) => n.component === UNKNOWN_COMPONENT)!
    expect(placeholder.props).toEqual({ component: `Gauge`, catalogId: A2UI_BASIC_CATALOG_ID })
    expect(c.expected.issues).toHaveLength(1)
  })

  test(`every basic component appears in the cases`, () => {
    const seen = new Set(cases.flatMap((c) => c.components.map((x) => x.component)))
    for (const name of [`Text`, `Image`, `Icon`, `Video`, `AudioPlayer`, `Row`, `Column`, `List`, `Card`, `Tabs`, `Modal`, `Divider`, `Button`, `TextField`, `CheckBox`, `ChoicePicker`, `Slider`, `DateTimeInput`])
      expect(seen.has(name), name).toBe(true)
  })
})

describe(`catalog-extension.json`, () => {
  const extension = defineExtension(extensionFixture.extension as unknown as ExtensionDef)
  const cases = extensionFixture.cases as { name: string; catalogId: string; components: FlatComponent[]; expected: unknown }[]

  test(`every extension case reduces byte for byte`, () => {
    for (const c of cases) {
      const result = reduceSurface(c.components, { catalogId: c.catalogId, extensions: [extension] })
      expect(canon(result), c.name).toBe(canon(c.expected))
    }
  })

  test(`an extension macro expands through core macros to natives plus its own natives`, () => {
    const c = cases[0]
    const { root } = reduceSurface(c.components, { catalogId: c.catalogId, extensions: [extension] })
    const kinds = new Set<string>()
    walk(root, (n) => kinds.add(n.component))
    expect(kinds.has(`StatCard`)).toBe(false)
    expect(kinds.has(`TrendLine`)).toBe(true)
    expect(kinds.has(`Box`)).toBe(true)
  })
})

describe(`kitchen-sink.json`, () => {
  test(`uses every visible component at least once`, () => {
    const seen = new Set<string>()
    const visit = (n: NestedNode) => {
      seen.add(n.component)
      for (const slot of Object.values(n.slots ?? {})) visit(slot)
      ;(n.children ?? []).forEach(visit)
    }
    visit(kitchenSink as unknown as NestedNode)
    for (const name of componentNames()) expect(seen.has(name), name).toBe(true)
  })

  test(`reduces to the committed expansion with no issues and unique ids`, () => {
    const result = reduceNested(kitchenSink as unknown as NestedNode, { catalogId: CORE_CATALOG_ID })
    expect(result.issues).toEqual([])
    const { $comment: _c, ...expected } = kitchenExpanded as unknown as { $comment: string; root: UiNode; issues: unknown[] }
    expect(canon(result)).toBe(canon(expected))
    const ids = preorder(result.root)
    expect(new Set(ids).size).toBe(ids.length)
    walk(result.root, (n) => expect(coreCatalog.components[n.component]?.kind, n.id).toBe(`native`))
  })
})
