import { describe, expect, it } from "vitest"
import { extractMentionEmails } from "@/lib/mention-refs"

describe(`extractMentionEmails`, () => {
  it(`extracts a single mention`, () => {
    expect(extractMentionEmails(`ping @ada@example.com about this`)).toEqual([
      `ada@example.com`,
    ])
  })

  it(`extracts multiple unique mentions and dedupes repeats`, () => {
    expect(
      extractMentionEmails(
        `@ada@example.com and @grace@example.com (cc @ada@example.com)`
      )
    ).toEqual([`ada@example.com`, `grace@example.com`])
  })

  it(`lowercases emails (matching is case-insensitive)`, () => {
    expect(extractMentionEmails(`hey @Ada@Example.COM`)).toEqual([
      `ada@example.com`,
    ])
  })

  it(`matches at start of text and start of line`, () => {
    expect(extractMentionEmails(`@ada@example.com first`)).toEqual([
      `ada@example.com`,
    ])
    expect(extractMentionEmails(`line one\n@ada@example.com second`)).toEqual([
      `ada@example.com`,
    ])
  })

  it(`ignores bare handles and plain emails`, () => {
    expect(extractMentionEmails(`hey @ada`)).toEqual([])
    expect(extractMentionEmails(`mail me at ada@example.com`)).toEqual([])
  })
})
