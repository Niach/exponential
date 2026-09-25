import type { TeamInvite } from "@/db/schema"

// EXP-630 placeholder members: a member whose email invite is still
// unaccepted has not joined yet. `pending` = the link still works, `expired`
// = it lapsed or was revoked (the roster row stays until removed; "Resend
// invite" mints a fresh link, at a corrected address if need be).
//
// EXP-1076 adds `unsent`: the Linear import seats everyone it found so
// attributions land, but nobody was ever asked to join — those rows carry
// `sent_at` NULL (and an already-lapsed `expires_at`, so expiry readers treat
// the token as dead). "Invite expired" would be a lie there; it reads "Not
// invited" and offers a first send.
export type PlaceholderStatus = `unsent` | `pending` | `expired`

// A member may hold several rows (a superseded link, then a fresh one): the
// best news wins — a live link outranks a lapsed one, and any issued link
// outranks "never invited".
const RANK: Record<PlaceholderStatus, number> = {
  pending: 2,
  expired: 1,
  unsent: 0,
}

export function placeholderStatuses(
  invites: Pick<
    TeamInvite,
    `placeholderUserId` | `acceptedAt` | `expiresAt` | `sentAt`
  >[],
  now: Date = new Date()
): Map<string, PlaceholderStatus> {
  const result = new Map<string, PlaceholderStatus>()
  for (const invite of invites) {
    if (!invite.placeholderUserId || invite.acceptedAt) continue
    // Never sent ⇒ never expired: the import stamps `expires_at = created_at`
    // on purpose, so expiry says nothing about this row.
    const status: PlaceholderStatus = !invite.sentAt
      ? `unsent`
      : new Date(invite.expiresAt) > now
        ? `pending`
        : `expired`
    const current = result.get(invite.placeholderUserId)
    if (current !== undefined && RANK[current] >= RANK[status]) continue
    result.set(invite.placeholderUserId, status)
  }
  return result
}

export const PLACEHOLDER_LABELS: Record<PlaceholderStatus, string> = {
  unsent: `Not invited`,
  pending: `Invited`,
  expired: `Invite expired`,
}
