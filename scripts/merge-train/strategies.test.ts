import { describe, expect, it } from "bun:test"
import { rustChangelog, webChangelog } from "./changelog-shapes"
import { fileNumber, jsonUnion, mergeJson, prependUnion, sameContents } from "./strategies"

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

describe(`prepend-union (#LOGS-2: never a fused entry)`, () => {
  const entriesHaveOneId = (text: string) =>
    text.split(/\n\s*\}[,;]?\n/).every((chunk) => (chunk.match(/\bid: /g) ?? []).length <= 1)

  it(`three web entries prepended at one spot stay three entries, the later unit's first`, () => {
    const base = webChangelog([`a`])
    const two = prependUnion(base, webChangelog([`b`, `a`]), webChangelog([`c`, `a`]))
    expect(two).toBe(webChangelog([`c`, `b`, `a`]))
    const three = prependUnion(base, two!, webChangelog([`d`, `a`]))
    expect(three).toBe(webChangelog([`d`, `c`, `b`, `a`]))
    expect(entriesHaveOneId(three!)).toBe(true)
  })

  it(`the Rust mirror: both sides' PREVIOUS_233 = the later one renumbered, never a duplicate const`, () => {
    const base = rustChangelog(`a`, [[232, `z`]])
    const side = (id: string) => rustChangelog(id, [[233, `a`], [232, `z`]])
    const two = prependUnion(base, side(`b`), side(`c`))
    expect(two).toBe(rustChangelog(`c`, [[234, `b`], [233, `a`], [232, `z`]]))
    const three = prependUnion(base, two!, side(`d`))
    expect(three).toBe(rustChangelog(`d`, [[235, `c`], [234, `b`], [233, `a`], [232, `z`]]))
    expect(entriesHaveOneId(three!)).toBe(true)
  })

  it(`anything but two pure insertions at one point is a conflict, never a line union`, () => {
    const base = webChangelog([`a`, `z`])
    // an edit of a base line
    expect(prependUnion(base, webChangelog([`b`, `a`, `z`]), webChangelog([`c`, `a`, `z`]).replace(`title: \`A\``, `title: \`A2\``))).toBeNull()
    // two insertion points
    expect(prependUnion(base, webChangelog([`b`, `a`, `z`]), webChangelog([`a`, `c`, `z`]))).toBeNull()
    // a deletion
    expect(prependUnion(base, webChangelog([`b`, `a`, `z`]), webChangelog([`z`]))).toBeNull()
  })

  it(`a duplicate const that cannot be renumbered safely is a conflict`, () => {
    const base = rustChangelog(`a`, [[232, `z`]])
    const ours = rustChangelog(`b`, [[233, `a`], [232, `z`]])
    // theirs names its PREVIOUS_233 twice: no rename
    const twice = rustChangelog(`c`, [[233, `a`], [232, `z`]]).replace(`title: "C",`, `title: "C", // see PREVIOUS_233`)
    expect(prependUnion(base, ours, twice)).toBeNull()
    // an unnumbered name both declare
    const plain = (x: string) => `const A = 1\n${x}const END = 2\n`
    expect(prependUnion(plain(``), plain(`const NEW = 1\n`), plain(`const NEW = 2\n`))).toBeNull()
  })

  it(`one side unchanged takes the other`, () => {
    expect(prependUnion(`a\n`, `a\n`, `b\na\n`)).toBe(`b\na\n`)
    expect(prependUnion(`a\n`, `b\na\n`, `a\n`)).toBe(`b\na\n`)
  })
})
