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
  for (const variant of [{}, { multiple: true }, { searchable: true }, { multiple: true, searchable: true }, { triggerVariant: `row` }, { triggerVariant: `row`, searchable: true }, { triggerVariant: `row`, description: `Hint` }, { triggerVariant: `row`, searchable: true, description: `Hint` }, { description: `Hint` }, { searchable: true, description: `Hint` }, { triggerVariant: `row`, options: [{ label: `main`, value: `main`, trailing: `· default` }, ...OPTIONS] }, { triggerVariant: `pill` }, { triggerVariant: `pill`, searchable: true }, { triggerVariant: `pill`, description: `Hint` }, { triggerVariant: `pill`, disabled: true }]) {
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
    [{ triggerVariant: `pill` }, `start`, `262px`, ``],
    [{ triggerVariant: `pill`, searchable: true }, `start`, `262px`, ``],
  ]
  for (const [variant, align, width, minWidth] of cases)
    it(`${JSON.stringify(variant)}: ${align}, ${width || minWidth}, the cursor on the first option`, async () => {
      const { react, solid } = await both({ tree: sel({ ...variant, value: `sprint` }), theme: `exponential`, mode: `dark` })
      expect(solid).toEqual(react)
      expect(react).toEqual({ align, width, minWidth, items: [[`hover`, false], [`selected`, true], [`disabled`, false]] })
    })
  it(`multiple: chosen = selected + check, the cursor (the first CHOSEN option, the IDE's multi picker) = hover`, async () => {
    const { react, solid } = await both({ tree: sel({ multiple: true, value: [`sprint`] }) })
    expect(solid).toEqual(react)
    expect(react.items).toEqual([[null, false], [`selected hover`, true], [`disabled`, false]])
  })
  it(`an option's trailing reads on after its label in the popup row (#EXP-1629)`, async () => {
    for (const searchable of [false, true]) {
      const tree = sel({ triggerVariant: `row`, searchable, value: `sprint`, options: [{ label: `Backlog`, value: `backlog`, trailing: `· default` }, { label: `Sprint`, value: `sprint` }] })
      const r = await renderReact({ tree })
      let react: string[]
      try {
        await act(async () => {
          openIt(r.root)
          await settle()
        })
        react = Array.from(document.querySelectorAll(`.xui-Select-item`)).map((el) => el.textContent ?? ``)
      } finally {
        r.cleanup()
      }
      const so = await renderSolidSurface({ tree })
      try {
        openIt(so.root)
        await settle()
        expect(Array.from(document.querySelectorAll(`.xui-Select-item`)).map((el) => el.textContent ?? ``)).toEqual(react)
      } finally {
        so.cleanup()
      }
      expect(react).toEqual([`Backlog · default`, `Sprint`])
    }
  })
  // #EXP-1765: the pill's popup, opened by the POINTER, shows no cursor until
  // a key (ArrowDown lands on the first enabled option); the keyboard keeps it.
  const pointerOpen = (root: Element) => {
    const t = root.querySelector(`.xui-Select-trigger`)!
    t.dispatchEvent(Object.assign(new MouseEvent(`pointerdown`, { bubbles: true, cancelable: true, button: 0 }), { pointerType: `mouse`, pointerId: 1 }))
    t.dispatchEvent(new MouseEvent(`click`, { bubbles: true, cancelable: true, button: 0 }))
  }
  const arrowDown = () => (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent(`keydown`, { key: `ArrowDown`, bubbles: true, cancelable: true }))
  const hovers = () => Array.from(document.querySelectorAll(`.xui-Select-item`)).map((el) => el.getAttribute(`data-xs`))
  const pointerThenKey = async (c: ParityCase) => {
    const run = async (open: () => Promise<void>, key: () => Promise<void>) => {
      await open()
      const before = hovers()
      await key()
      return { before, after: hovers() }
    }
    const r = await renderReact(c)
    let react: { before: (string | null)[]; after: (string | null)[] }
    try {
      react = await run(
        () => act(async () => {
          pointerOpen(r.root)
          await settle()
        }),
        () => act(async () => {
          arrowDown()
          await settle()
        })
      )
    } finally {
      r.cleanup()
    }
    const so = await renderSolidSurface(c)
    try {
      const solid = await run(
        async () => {
          pointerOpen(so.root)
          await settle()
        },
        async () => {
          arrowDown()
          await settle()
        }
      )
      return { react, solid }
    } finally {
      so.cleanup()
    }
  }
  for (const searchable of [false, true])
    it(`pill${searchable ? ` searchable` : ``}: opened by the pointer, no cursor until ArrowDown (#EXP-1765)`, async () => {
      const { react, solid } = await pointerThenKey({ tree: sel({ triggerVariant: `pill`, searchable, value: `sprint` }), theme: `exponential` })
      expect(solid).toEqual(react)
      expect(react).toEqual({ before: [null, `selected`, `disabled`], after: [`hover`, `selected`, `disabled`] })
    })
  it(`field: opened by the pointer keeps the cursor (only a pill drops it)`, async () => {
    const { react, solid } = await pointerThenKey({ tree: sel({ value: `sprint` }), theme: `exponential` })
    expect(solid).toEqual(react)
    expect(react.before).toEqual([`hover`, `selected`, `disabled`])
  })
  it(`loading: the Skeleton's rows in place of the options (#EXP-1765)`, async () => {
    for (const searchable of [false, true]) {
      const { react, solid } = await both({ tree: sel({ triggerVariant: `pill`, searchable, loading: true, value: `sprint` }) })
      expect(solid).toEqual(react)
      expect(react.items).toEqual([])
      const r = await renderReact({ tree: sel({ triggerVariant: `pill`, searchable, loading: true }) })
      const read = () => ({ rows: document.querySelectorAll(`[data-xui-id="s.skeleton"] .xui-Skeleton-row`).length, empty: document.querySelectorAll(`.xui-Select-empty`).length, search: document.querySelectorAll(`.xui-Select-searchfield`).length })
      let reactDom: ReturnType<typeof read>
      try {
        await act(async () => {
          openIt(r.root)
          await settle()
        })
        reactDom = read()
      } finally {
        r.cleanup()
      }
      const so = await renderSolidSurface({ tree: sel({ triggerVariant: `pill`, searchable, loading: true }) })
      try {
        openIt(so.root)
        await settle()
        expect(read()).toEqual(reactDom)
      } finally {
        so.cleanup()
      }
      expect(reactDom).toEqual({ rows: 3, empty: 0, search: searchable ? 1 : 0 })
    }
  })
  it(`a mono option: the label in the mono type, its trailing its own run; searchPlaceholder + searchIcon (#EXP-1765)`, async () => {
    for (const searchable of [false, true]) {
      const tree = sel({ triggerVariant: `pill`, searchable, searchPlaceholder: `Search branches`, value: `dev`, options: [{ label: `main`, value: `main`, trailing: `default`, mono: true }, { label: `dev`, value: `dev`, trailing: `· old` }] })
      const read = () => ({
        items: Array.from(document.querySelectorAll(`.xui-Select-item`)).map((el) => [el.hasAttribute(`data-mono`), el.querySelector(`.xui-Select-trailing`)?.textContent ?? null, el.querySelector(`.xui-Select-trailing`)?.getAttribute(`aria-hidden`) ?? null, el.textContent]),
        placeholder: document.querySelector<HTMLInputElement>(`.xui-Select-searchfield`)?.placeholder ?? null,
        icon: document.querySelector(`.xui-Select-searchbox > svg`)?.getAttribute(`data-xui-part`) ?? null,
      })
      const r = await renderReact({ tree, theme: `exponential` })
      let react: ReturnType<typeof read>
      try {
        await act(async () => {
          openIt(r.root)
          await settle()
        })
        react = read()
      } finally {
        r.cleanup()
      }
      const so = await renderSolidSurface({ tree, theme: `exponential` })
      try {
        openIt(so.root)
        await settle()
        expect(read()).toEqual(react)
      } finally {
        so.cleanup()
      }
      expect(react.items).toEqual([[true, `default`, `true`, `maindefault`], [false, null, null, `dev · old`]])
      expect(react.placeholder).toBe(searchable ? `Search branches` : null)
      expect(react.icon).toBe(searchable ? `Select/searchIcon` : null)
    }
  })
  it(`pill search: Clear search only with text, a press empties the field and keeps focus (#EXP-1765)`, async () => {
    const tree = sel({ triggerVariant: `pill`, searchable: true })
    const flow = async (wrap: (f: () => Promise<void> | void) => Promise<void>, root: Element) => {
      const out: unknown[] = []
      await wrap(async () => {
        openIt(root)
        await settle()
      })
      const field = () => document.querySelector<HTMLInputElement>(`.xui-Select-searchfield`)!
      out.push(document.querySelector(`.xui-Select-clear`) === null)
      await wrap(async () => {
        const f = field()
        // React tracks the value: set it through the prototype's setter.
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, `value`)!.set!.call(f, `Ba`)
        f.dispatchEvent(new Event(`input`, { bubbles: true }))
        await settle()
      })
      const clear = document.querySelector<HTMLButtonElement>(`.xui-Select-clear`)
      out.push(clear?.getAttribute(`aria-label`) ?? null, clear?.style.width ?? null, clear?.querySelector(`svg`)?.getAttribute(`data-slot`) ?? null, clear?.querySelector(`svg`)?.getAttribute(`width`) ?? null, document.querySelectorAll(`.xui-Select-item`).length)
      await wrap(async () => {
        clear!.dispatchEvent(new MouseEvent(`mousedown`, { bubbles: true, cancelable: true }))
        clear!.click()
        await settle()
      })
      out.push(field().value, document.activeElement === field(), document.querySelector(`.xui-Select-clear`) === null, document.querySelectorAll(`.xui-Select-item`).length)
      return out
    }
    const r = await renderReact({ tree, theme: `exponential` })
    let react: unknown[]
    try {
      react = await flow((f) => act(async () => void (await f())), r.root)
    } finally {
      r.cleanup()
    }
    const so = await renderSolidSurface({ tree, theme: `exponential` })
    try {
      expect(await flow(async (f) => void (await f()), so.root)).toEqual(react)
    } finally {
      so.cleanup()
    }
    expect(react).toEqual([true, `Clear search`, `17.5px`, `Select.clear`, `10.5`, 1, ``, true, true, 3])
  })
  it(`a loading Select still opens from the pointer (#EXP-1765)`, async () => {
    for (const searchable of [false, true]) {
      const tree = sel({ triggerVariant: `pill`, searchable, loading: true })
      const read = (root: Element) => [root.querySelector(`.xui-Select-trigger`)!.getAttribute(`aria-expanded`), document.querySelectorAll(`[data-xui-id="s.skeleton"] .xui-Skeleton-row`).length]
      const r = await renderReact({ tree })
      let react: unknown[]
      try {
        await act(async () => {
          pointerOpen(r.root)
          await settle()
        })
        react = read(r.root)
      } finally {
        r.cleanup()
      }
      const so = await renderSolidSurface({ tree })
      try {
        pointerOpen(so.root)
        await settle()
        expect(read(so.root)).toEqual(react)
      } finally {
        so.cleanup()
      }
      expect(react).toEqual([`true`, 3])
    }
  })
  it(`multiple, nothing chosen: no cursor until the arrows reach for the list`, async () => {
    const { react, solid } = await both({ tree: sel({ multiple: true, value: [] }) })
    expect(solid).toEqual(react)
    expect(react.items).toEqual([[null, false], [null, false], [`disabled`, false]])
  })
})
