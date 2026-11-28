// F50: the core's font-family emission escapes what it quotes.

import { describe, expect, test } from "bun:test"
import { cssString, fontStack, styleToCss } from "./css"

describe(`fontStack`, () => {
  test(`quotes the family, then Inter Symbols, then its fallback list`, () => {
    expect(fontStack(`Inter`, {})).toBe(`"Inter", "Inter Symbols"`)
    expect(fontStack(`Inter`, { Inter: { fallback: `ui-sans-serif, sans-serif` } })).toBe(`"Inter", "Inter Symbols", ui-sans-serif, sans-serif`)
  })

  test(`a CSS generic family stays a keyword (VAPP-162: ui-serif)`, () => {
    expect(fontStack(`ui-serif`, { "ui-serif": { fallback: `Georgia, serif` } })).toBe(`ui-serif, "Inter Symbols", Georgia, serif`)
    expect(fontStack(`ui-monospace`, {})).toBe(`ui-monospace, "Inter Symbols"`)
  })

  test(`a quote or backslash in a family name never ends the CSS string`, () => {
    expect(cssString(`a"}`)).toBe(`"a\\"}"`)
    expect(cssString(`a\\b`)).toBe(`"a\\\\b"`)
    expect(fontStack(`a"}`, {})).toBe(`"a\\"}", "Inter Symbols"`)
    expect(styleToCss({ fontFamily: `Say "hi"` })[`font-family`]).toBe(`"Say \\"hi\\"", "Inter Symbols"`)
  })
})
