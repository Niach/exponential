import { TRPCError } from "@trpc/server"
import { describe, expect, it } from "vitest"
import {
  assertMatchingWorkspaceIds,
  assertWorkspaceAccess,
  buildWhereClause,
} from "@/lib/workspace-membership"

describe(`workspace membership helpers`, () => {
  it(`allows members with the required role`, () => {
    expect(() =>
      assertWorkspaceAccess(
        {
          role: `owner`,
          userId: `user-1`,
          workspaceId: `workspace-1`,
        },
        [`owner`]
      )
    ).not.toThrow()
  })

  it(`rejects missing membership or insufficient role`, () => {
    expect(() => assertWorkspaceAccess(undefined)).toThrow(TRPCError)
    expect(() =>
      assertWorkspaceAccess(
        {
          role: `member`,
          userId: `user-1`,
          workspaceId: `workspace-1`,
        },
        [`owner`]
      )
    ).toThrow(`Insufficient role`)
  })

  it(`rejects issue and label pairs from different workspaces`, () => {
    expect(() =>
      assertMatchingWorkspaceIds(`workspace-1`, `workspace-2`)
    ).toThrow(`Issue and label must belong to the same workspace`)
  })

  it(`builds a safe where clause for scoped ids`, () => {
    expect(buildWhereClause(`id`, [`user-1`, `user-2`])).toBe(
      `"id" IN ('user-1','user-2')`
    )
    expect(buildWhereClause(`id`, [])).toBe(
      `"id" = '00000000-0000-0000-0000-000000000000'`
    )
  })
})
