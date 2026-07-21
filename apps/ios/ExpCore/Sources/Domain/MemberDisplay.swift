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
///
/// A blank/whitespace-only `name` counts as missing → the email is shown
/// instead (web parity: `displayUserName` uses a truthy check). Apple-ID logins
/// arrive with `name == ""` (NOT NULL column) when Apple omits the name, so a
/// plain `name ?? email` would render an empty label.
public func memberDisplayName(_ user: UserEntity?, id: String?, generic: String = "Someone") -> String {
    if let user {
        if let name = user.name, !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return name
        }
        return user.email
    }
    if let id { return memberPseudonym(userId: id) }
    return generic
}

/// Up-to-two-letter initials for an avatar chip, derived from a user's display
/// name or email. Ports Android's `initialsFor` (ui/components/Avatars.kt): for
/// an email the local part before `@` is used; the base splits on spaces and
/// `._-+`; two-plus parts yield the first letter of the first two, otherwise the
/// first two characters. Never empty (falls back to "?").
public func memberInitials(_ user: UserEntity?, id: String?) -> String {
    let nameOrEmail = memberDisplayName(user, id: id, generic: "")
    let base = nameOrEmail.trimmingCharacters(in: .whitespacesAndNewlines)
    if base.isEmpty { return "?" }
    let local = base.contains("@") ? String(base.prefix(while: { $0 != "@" })) : base
    let parts = local
        .split(whereSeparator: { $0 == " " || $0 == "." || $0 == "_" || $0 == "-" || $0 == "+" })
        .filter { !$0.isEmpty }
    if parts.count >= 2 {
        let first = parts[0].prefix(1)
        let second = parts[1].prefix(1)
        return (first + second).uppercased()
    }
    if local.count >= 2 { return String(local.prefix(2)).uppercased() }
    if !local.isEmpty { return String(local.prefix(1)).uppercased() }
    return "?"
}
