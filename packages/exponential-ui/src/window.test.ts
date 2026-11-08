// VAPP-290: the Window macro (a window frame around one child) through the
// reference expander and the neutral theme: the chrome bar (34 tall, hidden
// from AT) with three 10 px dots at the start and the optional centred title,
// the child in the body, the group named by the title.

import { describe, expect, test } from "bun:test"
import { coreCatalog } from "./catalog"
import { expandMacros } from "./macros"
import { builtinTheme } from "./themes"
import { resolveRecipe } from "./theme"
import type { UiNode } from "./types"

const window = (props: Record<string, unknown>): UiNode =>
  expandMacros({ id: `w`, component: `Window`, props, children: [{ id: `c`, component: `Text`, props: { text: `Child` }, children: [] }] })

const byId = (root: UiNode, id: string): UiNode | undefined => {
  if (root.id === id) return root
  for (const child of root.children) {
    const hit = byId(child, id)
    if (hit) return hit
  }
  return undefined
}

describe(`Window`, () => {
  test(`a layout macro with one child`, () => {
    const def = coreCatalog.components.Window
    expect(def?.kind).toBe(`macro`)
    expect(def?.children).toBe(`one`)
    expect(Object.keys(def?.props ?? {})).toEqual([`title`, `style`])
  })

  test(`the chrome bar over the body; the group is named by the title`, () => {
    const root = window({ title: `exponential.at` })
    expect(root.component).toBe(`Box`)
    expect(root.accessibility).toMatchObject({ role: `group`, label: `exponential.at` })
    expect(root.style).toMatchObject({ display: `flex`, flexDirection: `column`, borderRadius: `$radius.lg`, overflow: `hidden` })
    expect(root.children.map((c) => c.id)).toEqual([`w.chrome`, `w.body`])
    const chrome = byId(root, `w.chrome`)!
    expect(chrome.accessibility?.hidden).toBe(true)
    expect(chrome.style).toMatchObject({ height: 34, flexDirection: `row`, alignItems: `center` })
    expect(chrome.children.map((c) => c.id)).toEqual([`w.chrome.dots`, `w.chrome.title`, `w.chrome.end`])
    const dots = byId(root, `w.chrome.dots`)!.children
    expect(dots.map((d) => d.id)).toEqual([`w.chrome.dots.dot.0`, `w.chrome.dots.dot.1`, `w.chrome.dots.dot.2`])
    for (const dot of dots) expect(dot.style).toMatchObject({ width: 10, height: 10, borderRadius: `$radius.full` })
    const title = byId(root, `w.chrome.title`)!
    expect(title.props).toMatchObject({ text: `exponential.at`, variant: `caption`, align: `center`, lines: 1 })
    // The dots and the end spacer grow alike, so the title sits in the centre.
    expect(byId(root, `w.chrome.end`)!.style).toMatchObject({ flexGrow: 1, flexBasis: 0 })
    const body = byId(root, `w.body`)!
    expect(body.style).toMatchObject({ flexGrow: 1, minHeight: 0 })
    expect(body.children.map((c) => c.id)).toEqual([`c`])
  })

  test(`no title = no caption and no name; the dots stay`, () => {
    const root = window({})
    expect(byId(root, `w.chrome.title`)).toBeUndefined()
    expect(root.accessibility?.label).toBeUndefined()
    expect(byId(root, `w.chrome.dots`)!.children).toHaveLength(3)
  })

  test(`the neutral frame: hairline border, card fill, soft shadow; muted dots and title`, () => {
    const theme = builtinTheme(`neutral`)!
    const part = (name: string) => resolveRecipe(theme, { component: `Window`, part: name, props: {}, states: [] }, `dark`) as Record<string, unknown>
    expect(part(`root`)).toMatchObject({ backgroundColor: `#171717`, borderWidth: 1 })
    expect(part(`root`).boxShadow).toBeDefined()
    expect(part(`chrome`)).toMatchObject({ borderBottomWidth: 1 })
    expect(part(`dot`)).toMatchObject({ backgroundColor: `#a1a1a1`, opacity: 0.3 })
    expect(part(`title`)).toMatchObject({ color: `#a1a1a1` })
  })
})
