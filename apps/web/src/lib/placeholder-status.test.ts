import { describe, expect, it } from "vitest"
import { PLACEHOLDER_LABELS, placeholderStatuses } from "@/lib/placeholder-status"

// EXP-630: what the Members list reads off the synced invites — a member is
// "invited, not joined" while an invite bound to it is unaccepted.
const NOW = new Date(`2026-09-24T10:00:00Z`)
const later = new Date(`2026-10-01T10:00:00Z`)
const earlier = new Date(`2026-09-20T10:00:00Z`)

describe(`placeholderStatuses`, () => {
  it(`badges a pending invite, an expired one, and nothing once accepted`, () => {
    const statuses = placeholderStatuses(
      [
        { placeholderUserId: `p-pending`, acceptedAt: null, expiresAt: later },
        { placeholderUserId: `p-expired`, acceptedAt: null, expiresAt: earlier },
        { placeholderUserId: `p-joined`, acceptedAt: earlier, expiresAt: later },
        { placeholderUserId: null, acceptedAt: null, expiresAt: later },
      ],
      NOW
    )
    expect([...statuses.entries()]).toEqual([
      [`p-pending`, `pending`],
      [`p-expired`, `expired`],
    ])
  })

  it(`lets a fresh link outrank the expired one it superseded, in either order`, () => {
    const rows = [
      { placeholderUserId: `p`, acceptedAt: null, expiresAt: earlier },
      { placeholderUserId: `p`, acceptedAt: null, expiresAt: later },
    ]
    expect(placeholderStatuses(rows, NOW).get(`p`)).toBe(`pending`)
    expect(placeholderStatuses([...rows].reverse(), NOW).get(`p`)).toBe(`pending`)
  })

  it(`labels both states`, () => {
    expect(PLACEHOLDER_LABELS).toEqual({ pending: `Invited`, expired: `Invite expired` })
  })
})
