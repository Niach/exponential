import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/reporter-reply.json"
import type { User } from "@/db/schema"
import { authorLabel, commentCaption } from "./format"

// SLOP-4: the comment card's name and caption follow the pinned ×4 spec
// (`fixtures/reporter-reply.json`); this locks the web against it.
const ada = { id: `u1`, name: `Ada Lovelace`, email: `ada@acme.test` } as User

describe(`authorLabel`, () => {
  it(`names a reporter comment after the submission's reporter`, () => {
    expect(
      authorLabel({ authorId: null, source: `reporter` }, undefined, `Emma Fischer`)
    ).toBe(`Emma Fischer`)
    expect(
      authorLabel({ authorId: null, source: `reporter` }, undefined, null)
    ).toBe(fixture.copy.anonymousName)
    expect(
      authorLabel({ authorId: null, source: `reporter` }, undefined, `  `)
    ).toBe(fixture.copy.anonymousName)
  })

  it(`names a member comment after its synced author`, () => {
    expect(authorLabel({ authorId: `u1`, source: `user` }, ada, `Emma`)).toBe(
      `Ada Lovelace`
    )
  })

  it(`reads "Former member" when a non-reporter author row is missing`, () => {
    expect(authorLabel({ authorId: `gone`, source: `user` }, undefined, null)).toBe(
      fixture.copy.formerMemberName
    )
    expect(authorLabel({ authorId: null, source: `mcp` }, undefined, null)).toBe(
      fixture.copy.formerMemberName
    )
  })
})

describe(`commentCaption`, () => {
  it(`is exactly one of via MCP, reporter, to reporter, or none`, () => {
    expect(commentCaption({ source: `user`, audience: `team` })).toBeNull()
    expect(commentCaption({ source: `mcp`, audience: `team` })).toBe(`via MCP`)
    expect(commentCaption({ source: `reporter`, audience: `reporter` })).toBe(
      fixture.copy.reporterCaption
    )
    expect(commentCaption({ source: `user`, audience: `reporter` })).toBe(
      fixture.copy.toReporterCaption
    )
  })
})
