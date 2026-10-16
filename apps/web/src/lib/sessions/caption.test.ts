import { describe, expect, it } from "vitest"
import { ASIDE_TITLE_MAX, asideTitle, CAPTION_MAX, normalizeCaption } from "./caption"

describe(`normalizeCaption (EXP-1331)`, () => {
  it(`trims to one line and clears blanks`, () => {
    expect(normalizeCaption(`  Running   tests\n now `)).toBe(`Running tests now`)
    expect(normalizeCaption(`   `)).toBeNull()
    expect(normalizeCaption(null)).toBeNull()
  })

  it(`caps at 80 chars`, () => {
    const capped = normalizeCaption(`x`.repeat(200))!
    expect(capped).toHaveLength(CAPTION_MAX)
    expect(capped.endsWith(`…`)).toBe(true)
  })
})

describe(`asideTitle (EXP-1330)`, () => {
  it(`is the question's first line, capped`, () => {
    expect(asideTitle(`  why is CI red?\nmore context`)).toBe(`why is CI red?`)
    expect(asideTitle(`q`.repeat(300))).toHaveLength(ASIDE_TITLE_MAX)
  })
})
