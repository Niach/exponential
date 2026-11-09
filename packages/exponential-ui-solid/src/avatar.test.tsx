// #EXP-1629: the Solid Avatar replays fixtures/avatar.json (the cross-client rule):
// a seed paints `palette[fnv1a32(seed) % 8]` at 20% behind initials in it,
// no seed (absent or empty) leaves the recipe's muted fallback, the name
// reduces to the fixture's initials, and the name is never the seed.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/avatar.json"
import { ExponentialSurface } from "./surface"

afterEach(cleanup)

const rule = fixture as unknown as { palette: string[]; fillAlpha: number; hues: [string, number][]; initials: [string, string][] }

/** The hex → the rgb() jsdom normalises a style colour to. */
const rgb = (hex: string) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(`, `)})`

function paint(nodes: { id: string; props: Record<string, unknown> }[]) {
  const tree = { id: `root`, component: `Stack`, children: nodes.map((n) => ({ ...n, component: `Avatar` })) } as unknown as NestedNode
  const r = reduceNested(tree, { catalogId: CORE_CATALOG_ID })
  expect(r.issues).toEqual([])
  const { container } = render(() => <ExponentialSurface root={r.root} templates={r.templates} theme="neutral" id="av" />)
  return (id: string) => container.querySelector<HTMLElement>(`[data-xui-id="${id}"] .xui-Avatar-fallback`)!
}

describe(`Avatar (fixtures/avatar.json)`, () => {
  it(`a seed paints its palette hue at the fill alpha, the initials in it`, () => {
    const seeded = rule.hues.filter(([seed]) => seed !== ``)
    const fallback = paint(seeded.map(([seed], i) => ({ id: `a${i}`, props: { name: `Jane Doe`, seed } })))
    seeded.forEach(([, index], i) => {
      const hex = rule.palette[index]!
      const el = fallback(`a${i}`)
      expect(el.style.color).toBe(rgb(hex))
      expect(el.style.backgroundColor).toBe(`color-mix(in srgb, ${rgb(hex)} ${rule.fillAlpha * 100}%, transparent)`)
    })
  })

  it(`no seed (absent or empty) leaves the muted recipe; the name is never the seed`, () => {
    const fallback = paint([
      { id: `none`, props: { name: `Jane Doe` } },
      { id: `empty`, props: { name: `Jane Doe`, seed: `` } },
    ])
    for (const id of [`none`, `empty`]) expect(fallback(id).getAttribute(`style`)).toBeNull()
  })

  it(`the name reduces to the fixture's initials`, () => {
    const fallback = paint(rule.initials.map(([name], i) => ({ id: `n${i}`, props: { name } })))
    rule.initials.forEach(([, letters], i) => expect(fallback(`n${i}`).textContent).toBe(letters))
  })

  it(`xl sizes root and letters through the recipe`, () => {
    const fallback = paint([{ id: `xl`, props: { name: `Jane Doe`, seed: `demo-mira`, size: `xl` } }])
    const el = fallback(`xl`)
    expect(el.dataset.rSize).toBe(`xl`)
    expect(el.parentElement!.dataset.rSize).toBe(`xl`)
  })
})
