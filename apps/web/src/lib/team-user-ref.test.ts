import { describe, expect, it } from "vitest"
import { teamUserRefKinds } from "@/lib/team-user-ref"

// #FEED-218: what a member reference may match at the MCP boundary.
describe(`teamUserRefKinds`, () => {
  it(`a user id only matches as a user id`, () => {
    expect(teamUserRefKinds(`ptobS12jrytMAoSNc7Jzi33t3aS8ucan`)).toEqual({ userId: true, memberId: false, email: false })
  })
  it(`a UUID may be a member id`, () => {
    expect(teamUserRefKinds(`0e03f8c4-a114-414e-aa7d-47e4347ebdb6`)).toEqual({ userId: true, memberId: true, email: false })
  })
  it(`an @ makes it an email`, () => {
    expect(teamUserRefKinds(`ann@example.com`)).toEqual({ userId: true, memberId: false, email: true })
  })
})
