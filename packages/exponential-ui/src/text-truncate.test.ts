import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/text-truncate.json" with { type: "json" }
import { middleTruncateWidth } from "./text-truncate"

describe(`#EXP-1778 fixtures/text-truncate.json`, () => {
  for (const c of fixture.cases) {
    test(c.name, () => {
      const chars = (c.widths as { chars?: Record<string, number> }).chars ?? {}
      const measure = (ch: string) => chars[ch] ?? c.widths.default
      expect(middleTruncateWidth(c.text, c.width, measure, fixture.affix)).toBe(c.shown)
    })
  }
})
