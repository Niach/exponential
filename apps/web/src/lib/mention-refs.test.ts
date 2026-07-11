import { describe, expect, it } from "vitest"
import {
  extractMentionEmails,
  replaceMentionTokens,
} from "@/lib/mention-refs"

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

// The public-board scrub: member mentions must never hand real emails to
// anonymous visitors (REV-18).
describe(`replaceMentionTokens`, () => {
  const label = (email: string) =>
    email === `ada@example.com` ? `Member A1B2` : null

  it(`replaces resolved member mentions with the anonymized label`, () => {
    expect(replaceMentionTokens(`ping @ada@example.com please`, label)).toBe(
      `ping Member A1B2 please`
    )
  })

  it(`resolves case-insensitively (tokens are lowercase-normalized)`, () => {
    expect(replaceMentionTokens(`hey @Ada@Example.COM`, label)).toBe(
      `hey Member A1B2`
    )
  })

  it(`keeps unresolvable tokens verbatim (plain text the author typed)`, () => {
    expect(
      replaceMentionTokens(`cc @stranger@elsewhere.com`, label)
    ).toBe(`cc @stranger@elsewhere.com`)
  })

  it(`replaces every occurrence across the text`, () => {
    expect(
      replaceMentionTokens(
        `@ada@example.com then again @ada@example.com`,
        label
      )
    ).toBe(`Member A1B2 then again Member A1B2`)
  })

  it(`leaves plain emails (no @ prefix) untouched`, () => {
    expect(replaceMentionTokens(`mail ada@example.com`, label)).toBe(
      `mail ada@example.com`
    )
  })
})
