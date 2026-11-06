import { describe, expect, test } from "bun:test"

import fixture from "../../fixtures/markdown-render.json" with { type: "json" }
import { parseMarkdown } from "./markdown"
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
