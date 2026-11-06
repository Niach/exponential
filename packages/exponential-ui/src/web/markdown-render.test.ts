import { describe, expect, test } from "bun:test"

import fixture from "../../fixtures/markdown-render.json" with { type: "json" }
import { parseMarkdown, resetMarkdownStreamMemo, type MarkdownOptions } from "./markdown"
import { normalMarkdown } from "./markdown-normal"

// #VAPP-258: the golden render fixture, the web parser's side (React and
// Solid share it). A pending case must still differ: the list only shrinks.
const pending = new Set((fixture.pending as Record<string, string[]>).web ?? [])

describe(`markdown-render.json (web)`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      const actual = normalMarkdown(parseMarkdown(entry.text))
      if (pending.has(entry.name)) expect(actual).not.toEqual(entry.blocks as never)
      else expect(actual).toEqual(entry.blocks as never)
    })
  }
})

// #VAPP-258 step 5: host options, and a document streamed in prefix by
// prefix (in order: the memo reuses the stable blocks; cold: the same).
describe(`markdown-render.json optionCases (web)`, () => {
  for (const entry of fixture.optionCases) {
    test(entry.name, () => {
      expect(normalMarkdown(parseMarkdown(entry.text, entry.options as MarkdownOptions))).toEqual(entry.blocks as never)
    })
  }
})

describe(`markdown-render.json streamingCases (web)`, () => {
  for (const entry of fixture.streamingCases) {
    const options = { ...((entry as { options?: MarkdownOptions }).options ?? {}), streaming: true }
    test(entry.name, () => {
      resetMarkdownStreamMemo()
      entry.steps.forEach((blocks, i) => {
        const prefix = entry.text.slice(0, i + 1)
        expect([prefix, normalMarkdown(parseMarkdown(prefix, options))]).toEqual([prefix, blocks as never])
      })
      entry.steps.forEach((blocks, i) => {
        resetMarkdownStreamMemo()
        expect(normalMarkdown(parseMarkdown(entry.text.slice(0, i + 1), options))).toEqual(blocks as never)
      })
    })
  }
  test(`a finished block keeps its object while the text grows`, () => {
    resetMarkdownStreamMemo()
    const first = parseMarkdown(`One **a**\n\nTwo`, { streaming: true })[0]
    expect(parseMarkdown(`One **a**\n\nTwo and more`, { streaming: true })[0]).toBe(first!)
  })
  test(`streaming off parses the unfinished text as written`, () => {
    expect(normalMarkdown(parseMarkdown(`Say **bo`))).toEqual([{ kind: `paragraph`, runs: [{ text: `Say **bo` }] }])
  })
})
