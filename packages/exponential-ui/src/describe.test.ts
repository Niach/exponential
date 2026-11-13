import { describe, expect, test } from "bun:test"
import { describeSurface } from "./describe"
import type { UiNode } from "./types"

const node = (id: string, component: string, props: Record<string, unknown> = {}, children: UiNode[] = [], extra: Partial<UiNode> = {}): UiNode => ({ id, component, props, children, ...extra })

describe(`describeSurface (#FEED-218)`, () => {
  test(`binds text, counts data arrays and expands a bound list's first items`, () => {
    const root = node(`root`, `Stack`, {}, [node(`title`, `Heading`, { text: { "@path": `/title` } }), node(`list`, `List`, {}, [], { template: { component: `row`, path: `/entries/rows` } })])
    const templates = { row: node(`row`, `Row`, { title: { "@path": `name` }, subtitle: { "@path": `hours` } }) }
    const data = { title: `Arbeitsstunden`, entries: { rows: [{ name: `Mo`, hours: `8` }, { name: `Di`, hours: `7` }, { name: `Mi`, hours: `6` }] } }
    const { outline, issues } = describeSurface(root, data, { templates })
    expect(outline).toBe(
      [
        `Stack#root`,
        `  Heading#title text="Arbeitsstunden"`,
        `  List#list`,
        `    × 3 row (first 2)`,
        `    Row#row title="Mo" subtitle="8"`,
        `    Row#row title="Di" subtitle="7"`,
      ].join(`\n`)
    )
    expect(issues).toEqual([])
  })

  test(`an unbound list, an empty list and a missing template are issues`, () => {
    const root = node(`root`, `Stack`, {}, [
      node(`a`, `List`, {}, [], { template: { component: `row`, path: `/none` } }),
      node(`b`, `List`, {}, [], { template: { component: `row`, path: `/empty` } }),
      node(`c`, `List`, {}, [], { template: { component: `ghost`, path: `/empty` } }),
    ])
    const { issues } = describeSurface(root, { empty: [] }, { templates: { row: node(`row`, `Row`) } })
    expect(issues).toEqual([`a: nothing is bound at /none (a binding that resolved nothing, or a wrong path)`, `b: the list at /empty is empty`, `c: its item template ghost does not exist`])
  })

  test(`a failing expression is an issue and shows its default`, () => {
    const root = node(`t`, `Text`, { text: { "@call": `expr`, args: { expr: `1 / 0`, default: `–` } } })
    const { outline, issues } = describeSurface(root, {})
    expect(outline).toBe(`Text#t text="–"`)
    expect(issues).toEqual([`1 / 0: gave no value (the default is shown)`])
  })

  test(`caps the lines`, () => {
    const root = node(`root`, `Stack`, {}, Array.from({ length: 10 }, (_, i) => node(`t${i}`, `Text`, { text: `${i}` })))
    expect(describeSurface(root, {}, { maxLines: 3 }).outline.split(`\n`)).toEqual([`Stack#root`, `  Text#t0 text="0"`, `  Text#t1 text="1"`, `… 8 more lines`])
  })
})
