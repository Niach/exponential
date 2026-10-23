// FEED-128: DOM parity of the ChoiceTiles port with React's
// natives/choice-tiles.tsx (+ the headless ChoiceTilesFrame): every catalog
// fixture case, the specimen, more shapes (no pictures, a disabled option,
// columns, a multiple pick, a failing check inside a Form, a bound value),
// dark + rtl + forced states, and the DOM after a pick by click and by keys.

import { describe, expect, it } from "vitest"
import type { NestedNode } from "@exponential_app/ui"
import components from "@exponential_app/ui/fixtures/catalog-components.json"
import specimens from "@exponential_app/ui/fixtures/specimens.json"
import { expectDomParity, renderReact, renderSolidSurface, serializeDom, type ParityCase } from "../test/parity"

const cases = (components as unknown as { cases: { name: string; node: NestedNode }[] }).cases.filter((c) => c.node.component === `ChoiceTiles`)
const specimen = (specimens as unknown as { specimens: { component: string; node: NestedNode }[] }).specimens.find((s) => s.component === `ChoiceTiles`)!

const tiles = (props: Record<string, unknown>): NestedNode => ({ id: `tiles`, component: `ChoiceTiles`, props }) as NestedNode
const opts = [
  { label: `Alpha`, value: `a`, description: `First` },
  { label: `Beta`, value: `b` },
  { label: `Gamma`, value: `c`, description: `Third`, disabled: true },
  { label: `Delta`, value: `d`, description: `` },
]
const shapes: Record<string, NestedNode> = {
  "no pictures, a disabled option": tiles({ options: opts, value: `b` }),
  "nothing picked": tiles({ options: opts }),
  "three columns, multiple": tiles({ options: opts, type: `multiple`, value: `d,a`, columns: 3 }),
  "columns clamped (9 → 4, 0.4 → 1)": tiles({ options: opts, columns: 9 }),
  "columns 0.4": tiles({ options: opts, columns: 0.4 }),
  "one picture among plain tiles": tiles({ options: [{ label: `Pic`, value: `p`, image: `https://example.com/p.png` }, ...opts], value: `p` }),
  "a relative picture, denied by the media policy": tiles({ options: [{ label: `Rel`, value: `r`, image: `/api/attachments/1` }], value: `r` }),
  "disabled multiple": tiles({ options: opts, type: `multiple`, value: `a,b`, disabled: true }),
  "bound value": tiles({ options: opts, value: { path: `/pick` } }),
  "with accessibility": { ...tiles({ options: opts, value: `a` }), accessibility: { label: `Pick a layout` } } as NestedNode,
  "a style on the node": { ...tiles({ options: opts, value: `a` }), style: { gap: `$spacing.md`, padding: 4 } } as NestedNode,
}
const form: NestedNode = {
  id: `form`,
  component: `Form`,
  props: {},
  children: [{ id: `tiles`, component: `ChoiceTiles`, props: { name: `layout`, options: opts, validateOn: `change`, checks: [{ condition: { call: `required`, args: { value: { path: `/pick` } } }, message: `Pick one` }], value: { path: `/pick` } } }],
} as unknown as NestedNode

describe(`ChoiceTiles DOM parity with React`, () => {
  for (const c of cases) it(`fixture ${c.name}`, async () => expectDomParity({ tree: c.node }))
  it(`the ChoiceTiles specimen`, async () => expectDomParity({ tree: specimen.node }))
  it(`the ChoiceTiles specimen, exponential dark rtl`, async () => expectDomParity({ tree: specimen.node, theme: `exponential`, mode: `dark`, direction: `rtl` }))
  for (const [name, tree] of Object.entries(shapes)) {
    it(name, async () => expectDomParity({ tree, data: { pick: `c` } }))
    it(`${name}, orange dark rtl`, async () => expectDomParity({ tree, data: { pick: `c` }, theme: `orange`, mode: `dark`, direction: `rtl` }))
  }
  it(`inside a Form`, async () => expectDomParity({ tree: form, data: { pick: `` } }))
  for (const state of [`hover`, `focus`, `pressed`, `checked`, `open`, `disabled`, `invalid`]) {
    it(`forced ${state}`, async () => expectDomParity({ tree: cases[0].node, states: [state] }))
    it(`forced ${state}, no pictures`, async () => expectDomParity({ tree: shapes[`no pictures, a disabled option`], states: [state], theme: `exponential` }))
  }
})

/** The DOM after the same input on both sides. */
async function afterInput(c: ParityCase, input: (root: Element) => void, mustContain?: string): Promise<void> {
  const { act } = await import(`react`)
  const r = await renderReact(c)
  const s = await renderSolidSurface(c)
  try {
    await act(async () => input(r.root))
    input(s.root)
    await Promise.resolve()
    const react = serializeDom(r.root)
    if (mustContain) expect(react).toContain(mustContain)
    expect(serializeDom(s.root)).toBe(react)
    expect(s.root.ownerDocument.activeElement?.getAttribute(`data-value`) ?? null).toBe(r.root.ownerDocument.activeElement?.getAttribute(`data-value`) ?? null)
  } finally {
    r.cleanup()
    s.cleanup()
  }
}
const button = (root: Element, value: string) => root.querySelector<HTMLButtonElement>(`button[data-value="${value}"]`)!
const key = (root: Element, value: string, k: string) => button(root, value).dispatchEvent(new KeyboardEvent(`keydown`, { key: k, bubbles: true, cancelable: true }))

describe(`ChoiceTiles DOM parity with React after a pick`, () => {
  it(`single: click`, async () => afterInput({ tree: shapes[`no pictures, a disabled option`] }, (root) => button(root, `d`).click()))
  it(`multiple: click toggles in option order`, async () =>
    afterInput({ tree: shapes[`three columns, multiple`] }, (root) => {
      button(root, `b`).click()
    }))
  it(`multiple: click removes`, async () => afterInput({ tree: shapes[`three columns, multiple`] }, (root) => button(root, `a`).click()))
  it(`single: ArrowRight skips the disabled tile and picks`, async () => afterInput({ tree: shapes[`no pictures, a disabled option`] }, (root) => key(root, `b`, `ArrowRight`)))
  it(`single: ArrowLeft wraps`, async () => afterInput({ tree: shapes[`no pictures, a disabled option`] }, (root) => key(root, `b`, `ArrowUp`)))
  it(`single rtl: ArrowLeft moves forward`, async () => afterInput({ tree: shapes[`no pictures, a disabled option`], direction: `rtl` }, (root) => key(root, `b`, `ArrowLeft`)))
  it(`multiple: arrows move focus only`, async () => afterInput({ tree: shapes[`three columns, multiple`] }, (root) => key(root, `a`, `ArrowDown`)))
  it(`bound: a click writes the data model`, async () => afterInput({ tree: shapes[`bound value`], data: { pick: `a` } }, (root) => button(root, `b`).click()))
  it(`inside a Form, multiple: unpicking the last tile shows the failing check`, async () => {
    const multi = JSON.parse(JSON.stringify(form)) as { children: { props: Record<string, unknown> }[] }
    multi.children[0].props.type = `multiple`
    await afterInput({ tree: multi as unknown as NestedNode, data: { pick: `a` } }, (root) => button(root, `a`).click(), `ChoiceTiles/error`)
  })
  it(`inside a Form: a pick clears the failing check`, async () => afterInput({ tree: form, data: { pick: `` } }, (root) => button(root, `a`).click()))
})
