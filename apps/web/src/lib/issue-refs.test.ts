import { describe, expect, it } from "vitest"
import { extractIssueRefs } from "@/lib/issue-refs"

describe(`extractIssueRefs`, () => {
  it(`extracts a single reference`, () => {
    expect(extractIssueRefs(`duplicate of #MET-115, closing`)).toEqual([
      `MET-115`,
    ])
  })

  it(`extracts multiple unique references and dedupes repeats`, () => {
    expect(
      extractIssueRefs(`see #APP-1 and #MET-22 (also #APP-1 again)`)
    ).toEqual([`APP-1`, `MET-22`])
  })

  it(`uppercases identifiers (prefixes are stored uppercase)`, () => {
    expect(extractIssueRefs(`relates to #met-115`)).toEqual([`MET-115`])
  })

  it(`matches at start of text and start of line`, () => {
    expect(extractIssueRefs(`#MET-1 first`)).toEqual([`MET-1`])
    expect(extractIssueRefs(`line one\n#MET-2 second`)).toEqual([`MET-2`])
  })

  it(`ignores tokens glued to a preceding word or hash`, () => {
    expect(extractIssueRefs(`foo#MET-115`)).toEqual([])
    expect(extractIssueRefs(`##MET-115`)).toEqual([])
  })

  it(`ignores tokens that continue past the number`, () => {
    expect(extractIssueRefs(`#MET-115abc`)).toEqual([])
    expect(extractIssueRefs(`#MET-115-2`)).toEqual([])
  })

  it(`ignores plain hashes, headings and non-identifier tokens`, () => {
    expect(extractIssueRefs(`# Heading`)).toEqual([])
    expect(extractIssueRefs(`#123`)).toEqual([])
    expect(extractIssueRefs(`#MET-`)).toEqual([])
    expect(extractIssueRefs(`#-115`)).toEqual([])
    expect(extractIssueRefs(`no refs here`)).toEqual([])
  })

  it(`allows digits inside the prefix (but not as its first char)`, () => {
    expect(extractIssueRefs(`#A2C-9`)).toEqual([`A2C-9`])
  })

  it(`matches when followed by punctuation`, () => {
    expect(extractIssueRefs(`(#MET-3), #MET-4. #MET-5!`)).toEqual([
      `MET-3`,
      `MET-4`,
      `MET-5`,
    ])
  })
})
