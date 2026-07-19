import Foundation

// The server no longer syncs `users` rows for co-members of a public team
// (their profile stays server-side). When a userId can't be resolved to a
// synced row we still want a stable, non-identifying label instead of "Unknown"
// or a raw id — mirror that everywhere a member is shown.

/// Deterministic pseudonym for a user whose row isn't synced locally. Uses the
/// last 4 chars of the id so the same person reads the same across sessions.
public func memberPseudonym(userId: String) -> String {
    "Member \(userId.suffix(4).uppercased())"
}

/// Resolve a display name from an optional synced user plus its id. Falls back
/// to a deterministic pseudonym when the row is missing but the id is known,
/// and only to `generic` when there's no id at all (e.g. unassigned).
public func memberDisplayName(_ user: UserEntity?, id: String?, generic: String = "Someone") -> String {
    if let user { return user.name ?? user.email }
    if let id { return memberPseudonym(userId: id) }
    return generic
}
