import { describe, expect, it } from "bun:test"
import { fileNumber, jsonUnion, mergeJson, sameContents } from "./strategies"

describe(`json-union`, () => {
  it(`unions keys, the later unit wins a key both changed`, () => {
    expect(mergeJson({ a: 1, b: 1 }, { a: 2, b: 1, c: 1 }, { a: 3, b: 1, d: 1 })).toEqual({ a: 3, b: 1, c: 1, d: 1 })
  })
  it(`a key only one side changed keeps that change, deletions included`, () => {
    expect(mergeJson({ a: 1, b: 1 }, { a: 2 }, { a: 1, b: 1, c: 1 })).toEqual({ a: 2, c: 1 })
  })
  it(`recurses into objects and unions arrays`, () => {
    expect(
      mergeJson({ v: { x: [1] } }, { v: { x: [1, 2], o: 1 } }, { v: { x: [1, 3], t: 1 } })
    ).toEqual({ v: { x: [1, 2, 3], o: 1, t: 1 } })
  })
  it(`keeps the tip's indentation and final newline`, () => {
    expect(jsonUnion(`{}`, `{\n    "a": 1\n}\n`, `{"b": 2}`)).toBe(`{\n    "a": 1,\n    "b": 2\n}\n`)
  })
  it(`an add/add without a base`, () => {
    expect(jsonUnion(null, `{"a":[1]}`, `{"a":[2]}`)).toBe(`{\n  "a": [\n    1,\n    2\n  ]\n}`)
  })
})

describe(`renumber helpers`, () => {
  it(`reads a file's leading number`, () => {
    expect(fileNumber(`db/out/0042_brave_owl.sql`)).toBe(42)
    expect(fileNumber(`db/out/meta/_journal.json`)).toBeNull()
  })
  it(`compares contents, not names`, () => {
    expect(sameContents([`a\n`, `b`], [`b\n`, `a`])).toBe(true)
    expect(sameContents([`a`], [`a2`])).toBe(false)
  })
})
