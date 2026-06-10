import { describe, expect, it } from "vitest"
import { generateWidgetKey, isWidgetKeyFormat } from "./key"

describe(`widget keys`, () => {
  it(`generates keys matching the public format`, () => {
    for (let i = 0; i < 50; i++) {
      const key = generateWidgetKey()
      expect(isWidgetKeyFormat(key)).toBe(true)
      expect(key).toMatch(/^expw_/)
      expect(key).toHaveLength(37)
    }
  })

  it(`generates distinct keys`, () => {
    const keys = new Set(
      Array.from({ length: 100 }, () => generateWidgetKey())
    )
    expect(keys.size).toBe(100)
  })

  it(`rejects malformed keys`, () => {
    expect(isWidgetKeyFormat(``)).toBe(false)
    expect(isWidgetKeyFormat(`expk_${`a`.repeat(32)}`)).toBe(false)
    expect(isWidgetKeyFormat(`expw_${`a`.repeat(31)}`)).toBe(false)
    expect(isWidgetKeyFormat(`expw_${`a`.repeat(33)}`)).toBe(false)
    expect(isWidgetKeyFormat(`expw_${`a`.repeat(31)}!`)).toBe(false)
  })
})
