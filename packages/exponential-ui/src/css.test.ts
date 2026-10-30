// F50: the core's font-family emission escapes what it quotes.

import { describe, expect, test } from "bun:test"
import { cssString, fontStack, styleToCss } from "./css"

describe(`fontStack`, () => {
  test(`quotes the family and appends its fallback list`, () => {
    expect(fontStack(`Inter`, {})).toBe(`"Inter"`)
    expect(fontStack(`Inter`, { Inter: { fallback: `ui-sans-serif, sans-serif` } })).toBe(`"Inter", ui-sans-serif, sans-serif`)
  })

  test(`a CSS generic family stays a keyword (VAPP-162: ui-serif)`, () => {
    expect(fontStack(`ui-serif`, { "ui-serif": { fallback: `Georgia, serif` } })).toBe(`ui-serif, Georgia, serif`)
    expect(fontStack(`ui-monospace`, {})).toBe(`ui-monospace`)
  })

  test(`a quote or backslash in a family name never ends the CSS string`, () => {
    expect(cssString(`a"}`)).toBe(`"a\\"}"`)
    expect(cssString(`a\\b`)).toBe(`"a\\\\b"`)
    expect(fontStack(`a"}`, {})).toBe(`"a\\"}"`)
    expect(styleToCss({ fontFamily: `Say "hi"` })[`font-family`]).toBe(`"Say \\"hi\\""`)
  })
})
