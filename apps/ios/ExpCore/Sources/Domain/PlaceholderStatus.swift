import Foundation

// EXP-630 placeholder members: a member whose email invite is still
// unaccepted has not joined yet. `pending` = the link still works, `expired`
// = it lapsed or was revoked (the roster row stays until removed; resending
// mints a fresh link — a web-only surface, iOS only badges the row).
// Mirrors web `lib/placeholder-status.ts`.
public enum PlaceholderStatus: String, Sendable {
    case pending
    case expired

    /// The badge copy, byte-identical to web's `PLACEHOLDER_LABELS`.
    public var label: String {
        switch self {
        case .pending: return "Invited"
        case .expired: return "Invite expired"
        }
    }
}

/// The placeholder state per member id, read off the synced invites of one
/// team. An invite counts only while it is bound to a placeholder member AND
/// unaccepted; `expiresAt` in the future is `pending`, else `expired`.
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
        let expiresAt = WireTimestamps.parse(invite.expiresAt)
        let status: PlaceholderStatus =
            (expiresAt.map { $0 > now } ?? false) ? .pending : .expired
        // A pending link outranks an expired one for the same member.
        if result[placeholderUserId] == .pending { continue }
        result[placeholderUserId] = status
    }
    return result
}
