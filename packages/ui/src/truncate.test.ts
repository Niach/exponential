import { describe, expect, it } from "vitest"
import { middleTruncate } from "./truncate"

describe(`middleTruncate`, () => {
  it(`leaves short values untouched`, () => {
    expect(middleTruncate(`src/truncate.ts`, 40)).toBe(`src/truncate.ts`)
    expect(middleTruncate(``, 10)).toBe(``)
    // Exactly at the cap is not truncation.
    expect(middleTruncate(`abcde`, 5)).toBe(`abcde`)
  })

  it(`drops the MIDDLE, keeping both ends`, () => {
    const path = `apps/web/src/components/very/deep/nesting/file.tsx`
    const out = middleTruncate(path, 40)
    expect(out).toContain(`…`)
    expect([...out]).toHaveLength(40)
    expect(out.startsWith(`apps/`)).toBe(true)
    expect(out.endsWith(`file.tsx`)).toBe(true)
  })

  it(`never exceeds the cap`, () => {
    for (const max of [3, 4, 10, 21, 22]) {
      expect([...middleTruncate(`a`.repeat(200), max)]).toHaveLength(max)
    }
  })

  it(`a cap too small to hold an ellipsis returns the value whole`, () => {
    expect(middleTruncate(`abcdef`, 2)).toBe(`abcdef`)
    expect(middleTruncate(`abcdef`, 0)).toBe(`abcdef`)
    expect(middleTruncate(`abcdef`, -5)).toBe(`abcdef`)
  })

  it(`never cuts an astral character in half`, () => {
    expect(middleTruncate(`😀😀😀😀`, 3)).toBe(`😀…😀`)
  })
})
