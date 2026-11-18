// #EDE-95 on Solid (React's highlight.test.tsx): the search marks.
// The rendered DOM searches exactly like the normal form (every
// markdownCase's matches), and paintHighlights registers one CSS highlight
// range per match, the current one apart.

import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, highlightSpec, reduceNested, type NestedNode } from "@exponential_app/eui"
import { highlightPieces, HIGHLIGHT_NAMES } from "@exponential_app/eui/web"
import fixture from "@exponential_app/eui/fixtures/text-highlight.json"
import { ExponentialSurface } from "./surface"

afterEach(cleanup)

// jsdom has no CSS Custom Highlight API: a stand-in registry.
class FakeHighlight extends Set<Range> {}
const registry = new Map<string, FakeHighlight>()
beforeEach(() => {
  registry.clear()
  ;(window as unknown as { Highlight: unknown }).Highlight = FakeHighlight
  ;(window as unknown as { CSS: unknown }).CSS = { ...(window as unknown as { CSS?: object }).CSS, highlights: registry }
})

const paint = (node: NestedNode) => render(() => <ExponentialSurface root={reduceNested(node, { catalogId: CORE_CATALOG_ID }).root} theme="neutral" id="hl" />).container

const texts = (name: string) => [...(registry.get(name) ?? [])].map((r) => r.toString())

describe(`#EDE-95 highlight`, () => {
  for (const c of fixture.markdownCases) {
    it(`markdown: ${c.name}`, () => {
      const root = paint({ id: `md`, component: `Markdown`, props: { text: c.text } })
      const el = root.querySelector(`.xui-md`)!
      const pieces = highlightPieces(el, highlightSpec({ query: c.query })!, true)
      expect(pieces.length).toBe(c.count)
      cleanup()
    })
  }

  it(`a Markdown paints each match, the current one apart`, () => {
    paint({ id: `md`, component: `Markdown`, props: { text: `Find **foo** and *foo* in \`foo()\` [foo](https://foo.test)`, highlight: { query: `FOO`, current: 2 } } })
    expect(texts(HIGHLIGHT_NAMES.match)).toEqual([`foo`, `foo`, `foo`])
    expect(texts(HIGHLIGHT_NAMES.current)).toEqual([`foo`])
  })

  it(`a Row numbers title first: the subtitle's first match is current 1`, () => {
    paint({ id: `row`, component: `Row`, props: { title: `Printer offline`, subtitle: `The printer driver`, highlight: { query: `printer`, current: 1 } } })
    expect(texts(HIGHLIGHT_NAMES.match)).toEqual([`Printer`])
    expect(texts(HIGHLIGHT_NAMES.current)).toEqual([`printer`])
  })

  it(`a Text and a MessageBubble mark accents and case alike`, () => {
    paint({ id: `s`, component: `Stack`, children: [
      { id: `t`, component: `Text`, props: { text: `Résumé and RESUME`, highlight: { query: `resume` } } },
      { id: `b`, component: `MessageBubble`, props: { text: `a **resume**`, highlight: { query: `résumé`, current: 0 } } },
    ] })
    expect(texts(HIGHLIGHT_NAMES.match)).toEqual([`Résumé`, `RESUME`])
    expect(texts(HIGHLIGHT_NAMES.current)).toEqual([`resume`])
  })

  it(`no highlight, no ranges; removing the prop clears them`, () => {
    paint({ id: `t`, component: `Text`, props: { text: `foo` } })
    expect(texts(HIGHLIGHT_NAMES.match)).toEqual([])
  })
})
