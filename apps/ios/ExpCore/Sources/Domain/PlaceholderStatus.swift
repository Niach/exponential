import Foundation

// EXP-630 placeholder members: a member whose email invite is still
// unaccepted has not joined yet. `pending` = the link still works, `expired`
// = it lapsed or was revoked (the roster row stays until removed; resending
// mints a fresh link — a web-only surface, iOS only badges the row).
//
// EXP-1076 adds `unsent`: the Linear import seats everyone it found so
// attributions land, but nobody was ever asked to join — those rows carry
// `sent_at` NULL (and an already-lapsed `expires_at`, so expiry readers treat
// the token as dead). "Invite expired" would be a lie there; it reads "Not
// invited".
// Mirrors web `lib/placeholder-status.ts`.
public enum PlaceholderStatus: String, Sendable {
    case unsent
    case pending
    case expired

    /// The badge copy, byte-identical to web's `PLACEHOLDER_LABELS`.
    public var label: String {
        switch self {
        case .unsent: return "Not invited"
        case .pending: return "Invited"
        case .expired: return "Invite expired"
        }
    }

    /// Web's `RANK`: a member may hold several rows (a superseded link, then
    /// a fresh one) — the best news wins. A live link outranks a lapsed one,
    /// and any issued link outranks "never invited".
    fileprivate var rank: Int {
        switch self {
        case .pending: return 2
        case .expired: return 1
        case .unsent: return 0
        }
    }
}

/// The placeholder state per member id, read off the synced invites of one
/// team. An invite counts only while it is bound to a placeholder member AND
/// unaccepted; `sentAt` nil is `unsent` whatever the expiry says, else
/// `expiresAt` in the future is `pending`, else `expired`.
///
/// An unparseable `expires_at` reads as `expired` — the same fail-closed
/// choice web makes (an invalid `Date` never compares greater than `now`).
public func placeholderStatuses(
    invites: [TeamInviteEntity],
    now: Date = Date()
) -> [String: PlaceholderStatus] {
    var result: [String: PlaceholderStatus] = [:]
    for invite in invites {
        guard let placeholderUserId = invite.placeholderUserId,
              !placeholderUserId.isEmpty,
              invite.acceptedAt == nil
        else { continue }
        let status: PlaceholderStatus
        if invite.sentAt?.isEmpty ?? true {
            // Never sent ⇒ never expired: the import stamps
            // `expires_at = created_at` on purpose, so expiry says nothing
            // about this row.
            status = .unsent
        } else {
            let expiresAt = WireTimestamps.parse(invite.expiresAt)
            status = (expiresAt.map { $0 > now } ?? false) ? .pending : .expired
        }
        // pending > expired > unsent for the same member, in any input order.
        if let current = result[placeholderUserId], current.rank >= status.rank { continue }
        result[placeholderUserId] = status
    }
    return result
}
