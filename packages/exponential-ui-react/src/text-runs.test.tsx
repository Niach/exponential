// Round 4 (fixtures/text-runs.json): a Text's `runs` and `dropCap` on React.
// (Solid's text-runs.test.tsx replays the same cases.)

import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested, type NestedNode } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"

afterEach(cleanup)

class FakeHighlight extends Set<Range> {}
const registry = new Map<string, FakeHighlight>()
beforeEach(() => {
  registry.clear()
  ;(window as unknown as { Highlight: unknown }).Highlight = FakeHighlight
  ;(window as unknown as { CSS: unknown }).CSS = { ...(window as unknown as { CSS?: object }).CSS, highlights: registry }
})

const opened: string[] = []
const paint = (node: NestedNode) => render(<ExponentialSurface root={reduceNested(node, { catalogId: CORE_CATALOG_ID }).root} theme="neutral" id="tr" host={{ openUrl: (u: string) => void opened.push(u) }} />).container

describe(`round 4 Text runs`, () => {
  it(`paints each span over the Text's type`, () => {
    const root = paint({
      id: `t`,
      component: `Text`,
      props: { text: `ignored`, runs: [{ text: `Ship `, weight: `bold` }, { text: `today`, tone: `success`, mark: `underline` }, { text: ` x()`, mark: `code` }, { text: ` hi`, mark: `highlight` }, { text: ` gone`, mark: `strike` }] },
    })
    const el = root.querySelector(`[data-xui-id="t"]`)!
    expect(el.textContent).toBe(`Ship today x() hi gone`)
    const spans = [...el.querySelectorAll(`span, code`)] as HTMLElement[]
    expect(spans[0]!.style.fontWeight).toBe(`var(--xui-type-weight-bold)`)
    expect(spans[1]!.style.color).toBe(`var(--xui-color-success)`)
    expect(spans[1]!.style.textDecorationLine).toBe(`underline`)
    expect(spans[2]!.tagName).toBe(`CODE`)
    expect(spans[2]!.classList.contains(`xui-Markdown-code`)).toBe(true)
    expect(spans[3]!.style.backgroundColor).toBe(`var(--xui-color-match)`)
    expect(spans[4]!.style.textDecorationLine).toBe(`line-through`)
  })

  it(`a link opens through the host and never presses the parent; a denied one is text`, () => {
    opened.length = 0
    const root = paint({ id: `t`, component: `Text`, props: { runs: [{ text: `docs`, link: `https://exponential.at/docs` }, { text: ` bad`, link: `javascript:alert(1)` }] } })
    const links = root.querySelectorAll(`a[data-xui-text-link]`)
    expect(links.length).toBe(1)
    fireEvent.click(links[0]!)
    expect(opened).toEqual([`https://exponential.at/docs`])
  })

  it(`search marks run over the joined text`, () => {
    paint({ id: `t`, component: `Text`, props: { runs: [{ text: `fo` }, { text: `o bar`, weight: `bold` }], highlight: { query: `foo` } } })
    expect([...(registry.get(`xui-match`) ?? [])].map((r) => r.toString())).toEqual([`foo`])
  })

  it(`shimmer paints the joined text plain`, () => {
    const root = paint({ id: `t`, component: `Text`, props: { shimmer: true, runs: [{ text: `Work`, weight: `bold` }, { text: `ing` }] } })
    const el = root.querySelector(`[data-xui-id="t"]`)!
    expect(el.textContent).toBe(`Working`)
    expect(el.querySelector(`span`)).toBeNull()
  })

  it(`a drop cap floats the first letter over its lines`, () => {
    const root = paint({ id: `t`, component: `Text`, props: { text: `  Once upon a time`, dropCap: 2, highlight: { query: `once` } } })
    const el = root.querySelector(`[data-xui-id="t"]`)!
    expect(el.hasAttribute(`data-xui-dropcap`)).toBe(true)
    const cap = el.querySelector(`[data-xui-cap]`) as HTMLElement
    expect(cap.textContent).toBe(`O`)
    expect(cap.style.fontSize).toBe(`42px`)
    expect(cap.style.height).toBe(`40px`)
    expect(cap.style.lineHeight).toBe(`40px`)
    expect(el.textContent).toBe(`Once upon a time`)
    // The cap counts in the search but paints no fill.
    expect([...(registry.get(`xui-match`) ?? [])].map((r) => r.toString())).toEqual([`nce`])
  })

  it(`no drop cap under a lines clamp`, () => {
    const root = paint({ id: `t`, component: `Text`, props: { text: `Once`, dropCap: 3, lines: 2 } })
    expect(root.querySelector(`[data-xui-cap]`)).toBeNull()
  })
})
