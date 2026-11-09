// FEED-128: DOM parity of natives/select.tsx with React's (every catalog
// fixture case, forced states, dark + rtl, a placeholder, a Form with the
// hidden native select, invalid, an empty list, host-sourced options).

import { describe, expect, it } from "vitest"
import { act } from "react"
import type { NestedNode } from "@exponential_app/eui"
import { renderReact, renderSolidSurface, type ParityCase } from "../test/parity"
import { expectParity, fixtureCases } from "./inputs-parity"

const STATES = [`hover`, `focus`, `pressed`, `checked`, `open`, `disabled`, `invalid`]
const OPTIONS = [{ label: `Backlog`, value: `backlog`, icon: `star` }, { label: `Sprint`, value: `sprint` }, { label: `Done`, value: `done`, disabled: true }]
const sel = (props: Record<string, unknown>) => ({ id: `s`, component: `Select`, props: { label: `Board`, name: `board`, options: OPTIONS, ...props } }) as unknown as NestedNode

describe(`select: catalog fixture cases`, () => {
  for (const c of fixtureCases([`Select`])) it(c.name, async () => expectParity({ tree: c.node, theme: `neutral` }))
})

describe(`select: variants`, () => {
  const example = fixtureCases([`Select`])[0].node
  it(`every forced state`, async () => expectParity({ tree: example, theme: `exponential`, states: STATES }))
  it(`dark + rtl (orange)`, async () => expectParity({ tree: example, theme: `orange`, mode: `dark`, direction: `rtl` }))
  for (const variant of [{}, { multiple: true }, { searchable: true }, { multiple: true, searchable: true }, { triggerVariant: `row` }, { triggerVariant: `row`, searchable: true }]) {
    const tag = JSON.stringify(variant)
    it(`${tag}: placeholder (no value)`, async () => expectParity({ tree: sel({ ...variant, placeholder: `Pick one` }) }))
    it(`${tag}: a value not in the options`, async () => expectParity({ tree: sel({ ...variant, value: `nope` }) }))
    it(`${tag}: no options`, async () => expectParity({ tree: sel({ ...variant, options: [], value: `x` }) }))
    it(`${tag}: bound value`, async () => expectParity({ tree: sel({ ...variant, value: { "@path": `/v` } }), data: { v: variant && `multiple` in variant ? [`backlog`, `sprint`] : `sprint` } }))
    it(`${tag}: a comma list value`, async () => expectParity({ tree: sel({ ...variant, value: `backlog,sprint` }) }))
    it(`${tag}: inside a Form, invalid after submit`, async () =>
      expectParity(
        {
          tree: { id: `f`, component: `Form`, children: [sel({ ...variant, value: { "@path": `/v` }, checks: [{ condition: { "@call": `required`, args: { value: { "@path": `/v` } } }, message: `Pick a board` }] })] } as unknown as NestedNode,
          data: { v: `` },
          theme: `exponential`,
        },
        (root) => {
          root.querySelector(`form`)!.dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
        }
      ))
    it(`${tag}: inside a Form with a value`, async () => expectParity({ tree: { id: `f`, component: `Form`, children: [sel({ ...variant, value: `sprint` })] } as unknown as NestedNode }))
  }
})

// #EXP-1629: the open popup (the row's start-aligned 262 px under its
// value, an explicit align + popupWidth, the cursor on the FIRST enabled
// option as `hover` (a fresh pick), the chosen one `selected` with the
// check), the same on both. The serialised
// open DOM differs in what jsdom cannot model (focus-scope aria-hidden), so
// this compares the popup's contract, not the whole tree.
describe(`select: open popup`, () => {
  const settle = async () => {
    for (let n = 0; n < 5; n++) await new Promise((res) => setTimeout(res, 0))
  }
  const openIt = (root: Element) => {
    root.querySelector(`.xui-Select-trigger`)!.dispatchEvent(new KeyboardEvent(`keydown`, { key: `ArrowDown`, bubbles: true, cancelable: true }))
  }
  const summary = (root: Element) => {
    const content = root.ownerDocument.querySelector<HTMLElement>(`[data-xui-overlay="Select"]`)!
    const items = Array.from(content.querySelectorAll(`.xui-Select-item`))
    return { align: content.getAttribute(`data-align`), width: content.style.width, minWidth: content.style.minWidth, items: items.map((el) => [el.getAttribute(`data-xs`), Boolean(el.querySelector(`.xui-Select-check`))]) }
  }
  const both = async (c: ParityCase) => {
    const r = await renderReact(c)
    let react: ReturnType<typeof summary>
    try {
      await act(async () => {
        openIt(r.root)
        await settle()
      })
      react = summary(r.root)
    } finally {
      r.cleanup()
    }
    const so = await renderSolidSurface(c)
    try {
      openIt(so.root)
      await settle()
      return { react, solid: summary(so.root) }
    } finally {
      so.cleanup()
    }
  }
  const cases: [Record<string, unknown>, string, string, string][] = [
    [{}, `start`, ``, `var(--radix-select-trigger-width)`],
    [{ searchable: true }, `start`, ``, `var(--radix-popover-trigger-width)`],
    [{ triggerVariant: `row` }, `start`, `262px`, ``],
    [{ triggerVariant: `row`, searchable: true }, `start`, `262px`, ``],
    [{ triggerVariant: `row`, align: `center`, popupWidth: 300 }, `center`, `300px`, ``],
    [{ searchable: true, align: `end`, popupWidth: 200 }, `end`, `200px`, ``],
  ]
  for (const [variant, align, width, minWidth] of cases)
    it(`${JSON.stringify(variant)}: ${align}, ${width || minWidth}, the cursor on the first option`, async () => {
      const { react, solid } = await both({ tree: sel({ ...variant, value: `sprint` }), theme: `exponential`, mode: `dark` })
      expect(solid).toEqual(react)
      expect(react).toEqual({ align, width, minWidth, items: [[`hover`, false], [`selected`, true], [`disabled`, false]] })
    })
  it(`multiple: chosen = selected + check, the cursor (the first option) = hover`, async () => {
    const { react, solid } = await both({ tree: sel({ multiple: true, value: [`sprint`] }) })
    expect(solid).toEqual(react)
    expect(react.items).toEqual([[`hover`, false], [`selected`, true], [`disabled`, false]])
  })
})
