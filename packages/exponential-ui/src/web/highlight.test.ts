import { describe, expect, test } from "bun:test"
import fixture from "../../fixtures/text-highlight.json" with { type: "json" }
import { highlightRanges, HIGHLIGHT_BARRIER } from "../highlight"
import { markdownHighlightCount, markdownHighlightTexts, nodeHighlightCount } from "./highlight"
import { parseMarkdown } from "./markdown"
import { normalMarkdown } from "./markdown-normal"
import { linkedMarkdown } from "./markdown-linked"

describe(`#EDE-95 text-highlight.json markdownCases (web)`, () => {
  for (const c of fixture.markdownCases) {
    test(c.name, () => {
      const texts = markdownHighlightTexts(normalMarkdown(parseMarkdown(c.text)))
      const matches = texts.flatMap((t) => highlightRanges(t, c.query).map((r) => t.slice(r.start, r.end)))
      expect(matches).toEqual(c.matches)
      expect(markdownHighlightCount(c.text, c.query)).toBe(c.count)
    })
  }
  test(`a chip stays whole: one barrier, never matched`, () => {
    const host = { textLinks: (t: string) => (t.includes(`EXP-12`) ? [{ text: t.slice(0, t.indexOf(`EXP-12`)) }, { text: `EXP-12`, href: `https://x.test/i/EXP-12`, chip: { kind: `issue`, label: `Fix the printer` } }, { text: t.slice(t.indexOf(`EXP-12`) + 6) }] : null) }
    const tree = linkedMarkdown(host as never, parseMarkdown(`see EXP-12 printer`))
    expect(markdownHighlightTexts(normalMarkdown(tree))).toEqual([`see ${HIGHLIGHT_BARRIER} printer`])
    expect(markdownHighlightCount(``, `printer`, tree)).toBe(1)
    expect(markdownHighlightCount(``, `EXP`, tree)).toBe(0)
  })
  test(`nodeHighlightCount`, () => {
    expect(nodeHighlightCount(`Text`, { text: `a b a`, highlight: { query: `A` } })).toBe(2)
    expect(nodeHighlightCount(`Row`, { title: `Café`, subtitle: `cafe and CAFE`, highlight: { query: `cafe` } })).toBe(3)
    expect(nodeHighlightCount(`MessageBubble`, { text: `**foo** [x](https://foo.test)`, highlight: { query: `foo` } })).toBe(1)
    expect(nodeHighlightCount(`Text`, { text: `foo` })).toBe(0)
  })
})
