import { describe, expect, it } from "vitest"
import type { TeamMember } from "@/db/schema"
import {
  inviteExpiryLabel,
  membersInJoinOrder,
} from "@/components/team/members-section"

const member = (id: string, createdAt: string): TeamMember =>
  ({
    id,
    teamId: `t1`,
    userId: `u-${id}`,
    role: `member`,
    createdAt: new Date(createdAt),
    updatedAt: new Date(createdAt),
  }) as TeamMember

describe(`members settings`, () => {
  it(`lists members in join order, oldest first`, () => {
    const rows = membersInJoinOrder([
      member(`c`, `2026-03-01T00:00:00Z`),
      member(`a`, `2026-01-01T00:00:00Z`),
      member(`b`, `2026-02-01T00:00:00Z`),
    ])
    expect(rows.map((row) => row.id)).toEqual([`a`, `b`, `c`])
  })

  it(`prints invite expiry as "Expires Mon D"`, () => {
    expect(inviteExpiryLabel(new Date(2027, 5, 5, 12))).toBe(`Expires Jun 5`)
  })
})
