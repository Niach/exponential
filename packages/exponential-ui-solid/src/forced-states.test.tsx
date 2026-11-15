// #EXP-1781: FORCED interaction states in Solid (React's forced-states.test.tsx), replaying
// fixtures/forced-states.json through the DOM: the host's `forcedStates`
// (the LAST `forced` step) join the `data-xs` of the node and its parts,
// nothing else (no action, no input, no overlay). `host` steps are the
// core's (a real pointer); the DOM side has none to replay. Plus the
// specimens' state rows (`specimens.json` `states`) reach their parts.

import { afterEach, describe, expect, it } from "vitest"
import { createSignal } from "solid-js"
import { cleanup, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/forced-states.json"
import specimens from "@exponential_app/eui/fixtures/specimens.json"
import { ExponentialSurface } from "./surface"
import { expectDomParity } from "./test/parity"
import type { SurfaceActionEvent } from "./host"

afterEach(() => cleanup())

type Forced = Record<string, string[]>
interface Case {
  name: string
  tree: NestedNode
  steps: { forced?: Forced; host?: unknown }[]
  expect: { states: Record<string, string[]>; layers: number; events: unknown[] }
}
interface Specimen {
  id: string
  node: NestedNode
  states?: Forced
}

const FORCED_TOKENS = [`hover`, `pressed`, `focus`, `focus-visible`]
/** The forced-vocabulary tokens of an element's `data-xs`, in forced order. */
const forcedOn = (el: Element) => {
  const xs = (el.getAttribute(`data-xs`) ?? ``).split(` `)
  return FORCED_TOKENS.filter((s) => xs.includes(s))
}

function mount(tree: NestedNode, forcedStates: Forced | undefined) {
  const actions: SurfaceActionEvent[] = []
  const inputs: unknown[] = []
  const r = reduceNested(tree, { catalogId: CORE_CATALOG_ID })
  const host = { onAction: (e: SurfaceActionEvent) => void actions.push(e), onInput: (e: unknown) => void inputs.push(e) }
  const view = render(() => <ExponentialSurface root={r.root} templates={r.templates} theme="exponential" mode="light" id="fs" host={host} forcedStates={forcedStates} />)
  const byId = (id: string) => view.container.querySelector(`[data-xui-id="${id}"]`)
  return { ...view, actions, inputs, byId }
}

describe(`forced-states.json (Solid DOM)`, () => {
  for (const c of (fixture as unknown as { cases: Case[] }).cases) {
    it(c.name, () => {
      const forced = [...c.steps].reverse().find((s) => s.forced)?.forced
      const m = mount(c.tree, forced)
      let seen = 0
      for (const [id, states] of Object.entries(c.expect.states)) {
        const el = m.byId(id)
        if (!el) continue
        seen++
        expect(forcedOn(el), id).toEqual(states)
      }
      expect(seen).toBeGreaterThan(0)
      // Paint only: nothing fired, no overlay opened.
      expect(m.actions).toEqual([])
      expect(m.inputs).toEqual([])
      expect(document.querySelectorAll(`[role="tooltip"], [data-xui-part$="/content"][data-state="open"], [data-state="delayed-open"], [data-state="instant-open"]`).length).toBe(c.expect.layers)
      // No focus move.
      expect(document.activeElement === document.body || document.activeElement === null).toBe(true)
    })
  }

  it(`a new literal with the same entries keeps the painted states; an empty map restores the look`, () => {
    const tree = { id: `root`, component: `Box`, children: [{ id: `btn`, component: `Button`, props: { label: `Save` } }] } as unknown as NestedNode
    const r = reduceNested(tree, { catalogId: CORE_CATALOG_ID })
    const [forced, setForced] = createSignal<Forced>({ btn: [`hover`] })
    const view = render(() => <ExponentialSurface root={r.root} templates={r.templates} id="fs2" forcedStates={forced()} />)
    const btn = () => view.container.querySelector(`[data-xui-id="btn"]`)!
    expect(forcedOn(btn())).toEqual([`hover`])
    setForced({ btn: [`hover`] })
    expect(forcedOn(btn())).toEqual([`hover`])
    setForced({})
    expect(btn().getAttribute(`data-xs`)).toBeNull()
  })
})

describe(`specimens.json state rows reach their parts (Solid DOM)`, () => {
  const all = (specimens as unknown as { specimens: Specimen[] }).specimens
  const entry = (id: string) => all.find((s) => s.id === id)!

  it(`Tabs: s0_tabs.tab.1 hovers, its siblings do not`, () => {
    const m = mount(entry(`exponential-ui-tabs`).node, entry(`exponential-ui-tabs`).states)
    expect(forcedOn(m.byId(`s0_tabs.tab.1`)!)).toEqual([`hover`])
    expect(forcedOn(m.byId(`s0_tabs.tab.0`)!)).toEqual([])
  })

  it(`Radio (a host forcing an unplaced item): rd.item.1, the row and its circle, has a keyboard focus`, () => {
    const tree = { id: `root`, component: `Box`, children: [{ id: `rd`, component: `Radio`, props: { label: `Plan`, value: `a`, options: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }] } }] } as unknown as NestedNode
    const m = mount(tree, { "rd.item.1": [`focus`] })
    expect(forcedOn(m.byId(`rd.item.1`)!)).toEqual([`focus`, `focus-visible`])
    expect(forcedOn(m.byId(`rd.dot.1`)!)).toEqual([`focus`, `focus-visible`])
    expect(forcedOn(m.byId(`rd.dot.0`)!)).toEqual([])
    expect(forcedOn(m.byId(`rd`)!)).toEqual([])
  })

  it(`Segmented: item 1 of s0_segmented hovers; a host's pressed item (unplaced) paints only that item`, () => {
    const m = mount(entry(`exponential-ui-segmented`).node, entry(`exponential-ui-segmented`).states)
    const items = (id: string) => [...m.byId(id)!.querySelectorAll(`[data-xui-part="Segmented/item"]`)]
    expect(items(`s0_segmented`).map(forcedOn)[1]).toEqual([`hover`])
    expect(items(`s0_segmented`).map(forcedOn)[0]).toEqual([])
    cleanup()
    const tree = { id: `root`, component: `Box`, children: [{ id: `sg`, component: `Segmented`, props: { value: `a`, items: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }] } }] } as unknown as NestedNode
    const h = mount(tree, { "sg.item.1": [`pressed`] })
    expect([...h.byId(`sg`)!.querySelectorAll(`[data-xui-part="Segmented/item"]`)].map(forcedOn)).toEqual([[], [`pressed`]])
  })

  it(`every specimen's state keys that paint an element carry their states`, () => {
    const missing: string[] = []
    for (const s of all.filter((s) => s.states)) {
      const m = mount(s.node, s.states)
      for (const [id, states] of Object.entries(s.states!)) {
        const el = m.byId(id)
        if (!el) {
          missing.push(id)
          continue
        }
        const want = states.flatMap((x) => (x === `focus` ? [`focus`, `focus-visible`] : [x]))
        expect(forcedOn(el), `${s.id} ${id}`).toEqual(FORCED_TOKENS.filter((t) => want.includes(t)))
      }
      expect(m.actions, s.id).toEqual([])
      cleanup()
    }
    // Only the unplaced per-item parts (checked above by position) have no element.
    expect(missing).toEqual([`s0_segmented.item.1`])
  }, 60_000)
})

describe(`forced states: the DOM is React's`, () => {
  for (const c of (fixture as unknown as { cases: Case[] }).cases)
    it(c.name, async () => expectDomParity({ tree: c.tree, theme: `exponential`, forcedStates: [...c.steps].reverse().find((s) => s.forced)?.forced }))
  for (const s of (specimens as unknown as { specimens: Specimen[] }).specimens.filter((s) => s.states))
    it(`specimen ${s.id}`, async () => expectDomParity({ tree: s.node, theme: `exponential`, forcedStates: s.states }), 30_000)
  it(`Radio and Segmented items a host forces (unplaced per-item parts)`, async () =>
    expectDomParity({
      tree: { id: `root`, component: `Box`, children: [{ id: `rd`, component: `Radio`, props: { label: `Plan`, value: `a`, options: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }] } }, { id: `sg`, component: `Segmented`, props: { value: `a`, items: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }] } }, { id: `bar`, component: `Segmented`, props: { variant: `bar`, value: `a`, items: [{ label: `A`, value: `a`, icon: `star` }, { label: `B`, value: `b`, icon: `star` }] } }] } as unknown as NestedNode,
      theme: `exponential`,
      forcedStates: { "rd.item.1": [`focus`], "sg.item.1": [`pressed`], "bar.item.0": [`hover`] },
    }))
})
