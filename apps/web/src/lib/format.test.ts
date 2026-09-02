import { describe, expect, it } from "vitest"
import { middleTruncate } from "./format"

describe(`middleTruncate`, () => {
  it(`leaves values that already fit untouched`, () => {
    expect(middleTruncate(`src/lib/format.ts`, 40)).toBe(`src/lib/format.ts`)
    expect(middleTruncate(``, 10)).toBe(``)
    // Exactly at the cap is still a fit.
    expect(middleTruncate(`abcde`, 5)).toBe(`abcde`)
  })

  it(`keeps the filename visible on a long path`, () => {
    const path = `apps/web/src/components/helpdesk/support-inbox.tsx`
    const out = middleTruncate(path, 40)
    expect([...out]).toHaveLength(40)
    expect(out).toContain(`…`)
    expect(out.startsWith(`apps/`)).toBe(true)
    expect(out.endsWith(`support-inbox.tsx`)).toBe(true)
  })

  it(`never exceeds the cap`, () => {
    for (const max of [3, 4, 7, 12, 25]) {
      expect([...middleTruncate(`a`.repeat(200), max)]).toHaveLength(max)
    }
  })

  it(`gives up rather than emitting a lone ellipsis`, () => {
    expect(middleTruncate(`abcdef`, 2)).toBe(`abcdef`)
    expect(middleTruncate(`abcdef`, 0)).toBe(`abcdef`)
    expect(middleTruncate(`abcdef`, -5)).toBe(`abcdef`)
  })

  it(`never splits an astral character`, () => {
    // Four code points, eight UTF-16 units — a naive slice would emit
    // lone surrogates.
    expect(middleTruncate(`😀😀😀😀`, 3)).toBe(`😀…😀`)
  })
})
