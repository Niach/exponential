import { describe, expect, it } from "vitest"
import { parseFreezeNow } from "./freeze-now"

describe(`parseFreezeNow`, () => {
  it(`is off when the variable is unset, empty or whitespace`, () => {
    expect(parseFreezeNow(undefined)).toBeUndefined()
    expect(parseFreezeNow(``)).toBeUndefined()
    expect(parseFreezeNow(`   `)).toBeUndefined()
  })

  it(`reads epoch milliseconds`, () => {
    expect(parseFreezeNow(`1767225600000`)).toBe(1767225600000)
    expect(parseFreezeNow(` 1767225600000 `)).toBe(1767225600000)
  })

  it(`reads an ISO timestamp`, () => {
    expect(parseFreezeNow(`2026-01-01T00:00:00.000Z`)).toBe(
      Date.parse(`2026-01-01T00:00:00.000Z`)
    )
  })

  it(`throws on garbage instead of falling back to the real clock`, () => {
    // A silent fallback is the failure this exists to prevent: the run would
    // succeed and quietly write a differently-dated store.
    expect(() => parseFreezeNow(`yesterday`)).toThrow(/SCREENSHOT_FREEZE_NOW/)
    expect(() => parseFreezeNow(`0`)).toThrow(/epoch/)
    expect(() => parseFreezeNow(`-1`)).toThrow(/SCREENSHOT_FREEZE_NOW/)
  })
})
