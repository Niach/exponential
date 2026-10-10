import Foundation

/// The Members band ×4: JOIN order, oldest membership first (web
/// `membersInJoinOrder` in `members-section.tsx`). Ties break on the row id so
/// the order is stable across renders; an unparseable `created_at` sorts last.
public enum TeamMemberOrder {
    public static func joinOrder(_ members: [TeamMemberEntity]) -> [TeamMemberEntity] {
        let keyed: [(member: TeamMemberEntity, joined: Date)] = members.map { member in
            (member, WireTimestamps.parse(member.createdAt) ?? Date.distantFuture)
        }
        let sorted = keyed.sorted { (lhs: (member: TeamMemberEntity, joined: Date), rhs: (member: TeamMemberEntity, joined: Date)) -> Bool in
            if lhs.joined != rhs.joined { return lhs.joined < rhs.joined }
            return lhs.member.id < rhs.member.id
        }
        return sorted.map { $0.member }
    }
}

/// The pending-invite rows ×4: email or "Link invite" · role pill ·
/// "Expires Mon D" (web `InviteControls` + `inviteExpiryLabel`).
public enum PendingInvites {
    /// What a link invite reads as in place of an address (EXP-698).
    public static let linkInviteLabel = "Link invite"

    /// Pending = unaccepted AND unexpired. An expired placeholder invite is a
    /// marker on the member row, not a pending link; an `unsent` import seat
    /// is minted already expired, so this test keeps it out too.
    public static func pending(_ invites: [TeamInviteEntity], now: Date = Date()) -> [TeamInviteEntity] {
        invites.filter { invite in
            guard invite.acceptedAt == nil,
                  let expires = WireTimestamps.parse(invite.expiresAt)
            else { return false }
            return expires > now
        }
    }

    /// "Expires Oct 9" — the short `MMM d` date every client prints.
    public static func expiryLabel(
        _ expiresAt: String,
        timeZone: TimeZone = .current
    ) -> String? {
        guard let date = WireTimestamps.parse(expiresAt) else { return nil }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = timeZone
        formatter.dateFormat = "MMM d"
        return "Expires \(formatter.string(from: date))"
    }
}

extension SignInMethods {
    /// The Email code row's subtitle ×4: the address, plus
    /// "· Off on this server" while the server has no mail transport.
    public var emailCodeSubtitle: String {
        emailOtpEnabled ? email : "\(email) · Off on this server"
    }
}
