import type { TeamInvite } from "@/db/schema"

// EXP-630 placeholder members: a member whose email invite is still
// unaccepted has not joined yet. `pending` = the link still works, `expired`
// = it lapsed or was revoked (the roster row stays until removed; "Resend
// invite" mints a fresh link, at a corrected address if need be).
export type PlaceholderStatus = `pending` | `expired`

export function placeholderStatuses(
  invites: Pick<TeamInvite, `placeholderUserId` | `acceptedAt` | `expiresAt`>[],
  now: Date = new Date()
): Map<string, PlaceholderStatus> {
  const result = new Map<string, PlaceholderStatus>()
  for (const invite of invites) {
    if (!invite.placeholderUserId || invite.acceptedAt) continue
    const status: PlaceholderStatus =
      new Date(invite.expiresAt) > now ? `pending` : `expired`
    // A pending link outranks an expired one for the same member.
    if (result.get(invite.placeholderUserId) === `pending`) continue
    result.set(invite.placeholderUserId, status)
  }
  return result
}

export const PLACEHOLDER_LABELS: Record<PlaceholderStatus, string> = {
  pending: `Invited`,
  expired: `Invite expired`,
}
