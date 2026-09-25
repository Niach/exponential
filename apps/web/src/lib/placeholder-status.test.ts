import { describe, expect, it } from "vitest"
import { PLACEHOLDER_LABELS, placeholderStatuses } from "@/lib/placeholder-status"

// EXP-630: what the Members list reads off the synced invites — a member is
// "invited, not joined" while an invite bound to it is unaccepted.
// EXP-1076: an imported roster row nobody was ever sent a link for reads
// "Not invited", never "Invite expired".
const NOW = new Date(`2026-09-24T10:00:00Z`)
const later = new Date(`2026-10-01T10:00:00Z`)
const earlier = new Date(`2026-09-20T10:00:00Z`)

describe(`placeholderStatuses`, () => {
  it(`badges a pending invite, an expired one, and nothing once accepted`, () => {
    const statuses = placeholderStatuses(
      [
        { placeholderUserId: `p-pending`, acceptedAt: null, expiresAt: later, sentAt: earlier },
        { placeholderUserId: `p-expired`, acceptedAt: null, expiresAt: earlier, sentAt: earlier },
        { placeholderUserId: `p-joined`, acceptedAt: earlier, expiresAt: later, sentAt: earlier },
        { placeholderUserId: null, acceptedAt: null, expiresAt: later, sentAt: earlier },
      ],
      NOW
    )
    expect([...statuses.entries()]).toEqual([
      [`p-pending`, `pending`],
      [`p-expired`, `expired`],
    ])
  })

  it(`reads a never-sent row as unsent whatever its expiry says`, () => {
    // The import stamps expires_at = created_at, so the row is "expired" by
    // date from the moment it exists — the label must not say so.
    const statuses = placeholderStatuses(
      [
        { placeholderUserId: `p-import`, acceptedAt: null, expiresAt: earlier, sentAt: null },
        { placeholderUserId: `p-future`, acceptedAt: null, expiresAt: later, sentAt: null },
      ],
      NOW
    )
    expect(statuses.get(`p-import`)).toBe(`unsent`)
    expect(statuses.get(`p-future`)).toBe(`unsent`)
  })

  it(`lets a fresh link outrank the expired one it superseded, in either order`, () => {
    const rows = [
      { placeholderUserId: `p`, acceptedAt: null, expiresAt: earlier, sentAt: earlier },
      { placeholderUserId: `p`, acceptedAt: null, expiresAt: later, sentAt: earlier },
    ]
    expect(placeholderStatuses(rows, NOW).get(`p`)).toBe(`pending`)
    expect(placeholderStatuses([...rows].reverse(), NOW).get(`p`)).toBe(`pending`)
  })

  it(`ranks pending > expired > unsent for the same member, in either order`, () => {
    const unsent = { placeholderUserId: `p`, acceptedAt: null, expiresAt: earlier, sentAt: null }
    const expired = { placeholderUserId: `p`, acceptedAt: null, expiresAt: earlier, sentAt: earlier }
    const pending = { placeholderUserId: `p`, acceptedAt: null, expiresAt: later, sentAt: earlier }
    expect(placeholderStatuses([unsent, expired], NOW).get(`p`)).toBe(`expired`)
    expect(placeholderStatuses([expired, unsent], NOW).get(`p`)).toBe(`expired`)
    expect(placeholderStatuses([unsent, pending], NOW).get(`p`)).toBe(`pending`)
    expect(placeholderStatuses([pending, unsent], NOW).get(`p`)).toBe(`pending`)
    expect(placeholderStatuses([pending, expired, unsent], NOW).get(`p`)).toBe(`pending`)
  })

  it(`labels all three states`, () => {
    expect(PLACEHOLDER_LABELS).toEqual({
      unsent: `Not invited`,
      pending: `Invited`,
      expired: `Invite expired`,
    })
  })
})
