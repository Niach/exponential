import { TRPCError } from "@trpc/server"
import { describe, expect, it } from "vitest"
import {
  assertMatchingTeamIds,
  assertTeamAccess,
  buildWhereClause,
} from "@/lib/team-membership"

describe(`team membership helpers`, () => {
  it(`allows members with the required role`, () => {
    expect(() =>
      assertTeamAccess(
        {
          role: `owner`,
          userId: `user-1`,
          teamId: `team-1`,
        },
        [`owner`]
      )
    ).not.toThrow()
  })

  it(`rejects missing membership or insufficient role`, () => {
    expect(() => assertTeamAccess(undefined)).toThrow(TRPCError)
    expect(() =>
      assertTeamAccess(
        {
          role: `member`,
          userId: `user-1`,
          teamId: `team-1`,
        },
        [`owner`]
      )
    ).toThrow(`Insufficient role`)
  })

  it(`rejects issue and label pairs from different teams`, () => {
    expect(() =>
      assertMatchingTeamIds(`team-1`, `team-2`)
    ).toThrow(`Issue and label must belong to the same team`)
  })

  it(`builds a safe where clause for scoped ids`, () => {
    expect(buildWhereClause(`id`, [`user-1`, `user-2`])).toBe(
      `"id" IN ('user-1','user-2')`
    )
    expect(buildWhereClause(`id`, [])).toBe(
      `"id" = '00000000-0000-0000-0000-000000000000'`
    )
  })

  it(`sorts ids so the same set always yields the same where clause`, () => {
    // The where clause is part of Electric's shape identity — heap-order
    // flips between requests must not rotate the shape handle.
    expect(buildWhereClause(`id`, [`user-2`, `user-1`])).toBe(
      `"id" IN ('user-1','user-2')`
    )
    expect(buildWhereClause(`id`, [`user-1`, `user-2`])).toBe(
      buildWhereClause(`id`, [`user-2`, `user-1`])
    )
  })
})
