import { describe, expect, it } from "vitest"
import { guideSectionPage, parseGuideSection } from "@/components/guide-section-diff"

// EXP-1251 / EXP-1293: the Guide tab's `?section=` and the section page a
// Changes row opens.
describe(`parseGuideSection`, () => {
  it(`parses a section: a 1-based number, lead, other, all; garbage = none`, () => {
    expect(parseGuideSection(`3`)).toBe(3)
    expect(parseGuideSection(2)).toBe(2)
    expect(parseGuideSection(`lead`)).toBe(`lead`)
    expect(parseGuideSection(`other`)).toBe(`other`)
    expect(parseGuideSection(`all`)).toBe(`all`)
    expect(parseGuideSection(`0`)).toBeUndefined()
    expect(parseGuideSection(`1.5`)).toBeUndefined()
    expect(parseGuideSection(`x`)).toBeUndefined()
    expect(parseGuideSection(undefined)).toBeUndefined()
  })
})

describe(`guideSectionPage`, () => {
  const file = (path: string, additions: number, deletions: number, previousPath?: string) => ({
    path,
    previousPath,
    additions,
    deletions,
  })
  const files = [file(`a.ts`, 3, 1), file(`b.ts`, 5, 0, `old-b.ts`), file(`c.ts`, 1, 1)]
  const groups = [
    { topic: `Summary`, files: [] },
    { topic: `Model`, files: [`a.ts`] },
    { topic: `Paint`, files: [`old-b.ts`] },
  ]

  it(`opens a numbered section with its caption and covered files`, () => {
    expect(guideSectionPage(groups, files, 2)).toEqual({
      section: 2,
      caption: `02 / 02`,
      title: `Paint`,
      files: [files[1]],
      additions: 5,
      deletions: 0,
    })
  })

  it(`opens Other changes and the complete diff`, () => {
    expect(guideSectionPage(groups, files, `other`)?.files).toEqual([files[2]])
    expect(guideSectionPage(groups, files, `other`)?.title).toBe(`Other changes`)
    const all = guideSectionPage(groups, files, `all`)
    expect(all?.files).toHaveLength(3)
    expect(all?.caption).toBeNull()
    expect([all?.additions, all?.deletions]).toEqual([9, 2])
  })

  it(`is null for a stale section or while the diff loads`, () => {
    expect(guideSectionPage(groups, files, 7)).toBeNull()
    expect(guideSectionPage(groups, null, 1)).toBeNull()
    expect(
      guideSectionPage([{ topic: `Model`, files: [`a.ts`, `b.ts`, `c.ts`] }], files, `other`)
    ).toBeNull()
  })

  it(`with no report the whole diff is one Changes section`, () => {
    expect(guideSectionPage([], files, `other`)?.title).toBe(`Changes`)
  })
})
