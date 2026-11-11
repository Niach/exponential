// #EXP-1629: interactions-round1.json `menuPreview` + `menuRows` against the
// TS reference (src/menu-preview.ts) and the catalog; the painters replay
// the same cases (React, Solid, the core so gpui, SwiftUI and Compose).
import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/interactions-round1.json" with { type: "json" }
import { CORE_CATALOG_ID, coreCatalog } from "./catalog"
import { bindTree } from "./dynamic"
import { embedCard } from "./embed"
import { MENU_PREVIEW_CLOSE_MS, MENU_PREVIEW_IDLE, MENU_PREVIEW_OFFSET, MENU_PREVIEW_OPEN_MS, menuPreviewInput, menuPreviewTick, type MenuPreviewState } from "./menu-preview"
import { reduceNested } from "./reducer"
import type { NestedNode } from "./types"

type Item = { kind?: string; preview?: unknown; items?: Item[] }

/** The row at `key` (`"3.0"`) in `items`. */
function itemAt(items: Item[], key: string): Item | undefined {
  const [head, ...rest] = key.split(`.`).map(Number)
  const item = items[head!]
  return rest.length === 0 ? item : itemAt(item?.items ?? [], rest.join(`.`))
}

/** A key the state may hold: a row with a preview (never a submenu parent). */
export function previewKey(items: Item[], key: string | null): string | null {
  if (key === null) return null
  const item = itemAt(items, key)
  return item && item.kind !== `submenu` && item.preview !== undefined ? key : null
}

describe(`menuPreview`, () => {
  const items = fixture.menuPreview.menu.props.items as Item[]
  test(`the constants`, () => {
    expect([MENU_PREVIEW_OPEN_MS, MENU_PREVIEW_CLOSE_MS, MENU_PREVIEW_OFFSET]).toEqual([150, 80, 10])
  })
  test(`the menu and every preview reduce clean`, () => {
    expect(reduceNested(fixture.menuPreview.menu as unknown as NestedNode, { catalogId: CORE_CATALOG_ID }).issues).toEqual([])
    const all = [...items, ...items.flatMap((i) => i.items ?? [])].filter((i) => i.preview)
    expect(all.length).toBe(3)
    for (const i of all) {
      const card = embedCard(i.preview)
      expect(card).toBeDefined()
      expect(reduceNested(card!.root, { catalogId: CORE_CATALOG_ID }).issues).toEqual([])
    }
  })
  test(`a preview is DATA: the bind pass never reads inside it`, () => {
    const inner = { root: { id: `leak`, component: `Text`, props: { text: { "@path": `/secret` } } } }
    const root = reduceNested({ id: `m`, component: `Menu`, props: { label: `More`, items: [{ label: `A`, preview: inner }] } }, { catalogId: CORE_CATALOG_ID }).root
    const bound = bindTree(root, { secret: `Host secret` }) as unknown as { props: { items: Array<{ preview: unknown }> } }
    expect(bound.props.items[0]!.preview).toEqual(inner)
  })
  test(`the catalog: three optional item fields, the Menu's action event`, () => {
    const shape = (coreCatalog as unknown as { defs: Record<string, { properties: Record<string, { type: string; shape?: string; required?: boolean }> }> }).defs.menuItem!.properties
    expect(shape.identifier).toMatchObject({ type: `string` })
    expect(shape.detail).toMatchObject({ type: `string` })
    expect(shape.preview).toMatchObject({ type: `object` })
    expect(shape.preview!.shape).toBeUndefined()
    expect((coreCatalog.components.Menu as unknown as { events: string[] }).events).toEqual([`select`, `action`])
  })
  for (const c of fixture.menuPreview.cases) {
    test(c.name, () => {
      let s: MenuPreviewState = MENU_PREVIEW_IDLE
      let now = 0
      for (const [i, step] of c.steps.entries()) {
        const st = step as { hover?: string | null; overPreview?: boolean; highlight?: string | null; wait?: number; press?: string; shown: string | null }
        if (`hover` in st) s = menuPreviewInput(s, { hover: previewKey(items, st.hover ?? null) }, now)
        if (`overPreview` in st) s = menuPreviewInput(s, { over: st.overPreview! }, now)
        if (`highlight` in st) s = menuPreviewInput(s, { highlight: previewKey(items, st.highlight ?? null) }, now)
        if (st.wait !== undefined) {
          now += st.wait
          s = menuPreviewTick(s, now)
        }
        expect({ step: i, shown: s.shown }).toEqual({ step: i, shown: st.shown })
      }
    })
  }
})

describe(`menuRows`, () => {
  test(`every case reduces clean`, () => {
    for (const c of fixture.menuRows) {
      expect(reduceNested({ id: `m`, component: `Menu`, props: { label: `More`, items: c.items } } as unknown as NestedNode, { catalogId: CORE_CATALOG_ID }).issues).toEqual([])
    }
  })
})
