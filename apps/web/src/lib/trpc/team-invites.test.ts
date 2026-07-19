import { describe, expect, it } from "vitest"
import { inviteListSelection } from "@/lib/trpc/team-invites"

// REV-4: teamInvites.list is member-visible (and relayed verbatim by the
// MCP exponential_invites_list tool), so it must never return the invite
// bearer `token` — accept() is not recipient-bound, and a leaked owner-role
// token lets any member escalate to owner. The token's only surface is the
// `create` mutation response, to the owner who minted it.
describe(`teamInvites.list selection contract`, () => {
  it(`excludes the invite bearer token`, () => {
    expect(Object.keys(inviteListSelection)).not.toContain(`token`)
  })

  it(`selects exactly the member-visible invite fields`, () => {
    expect(Object.keys(inviteListSelection).sort()).toEqual([
      `acceptedAt`,
      `createdAt`,
      `expiresAt`,
      `id`,
      `invitedById`,
      `role`,
      `teamId`,
      `updatedAt`,
    ])
  })
})
