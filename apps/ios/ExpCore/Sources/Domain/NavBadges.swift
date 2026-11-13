import Foundation

/// #FEED-232: ONE count badge ×4: every nav destination says HOW MANY things
/// in it need the viewer, never a dot. The spec is
/// `packages/domain-contract/fixtures/nav-badges.json`'s `$comment`; this is
/// its iOS mirror (TS `nav-badges.ts`), replayed by `NavBadgesTests`.
public enum NavBadges {
    public enum Tone: String, Equatable, Sendable {
        case attention
        case muted
    }

    /// A destination's badge: `count` ≤ 0 = no badge.
    public struct Value: Equatable, Sendable {
        public let count: Int
        public let tone: Tone

        public init(count: Int, tone: Tone) {
            self.count = count
            self.tone = tone
        }

        public static let none = Value(count: 0, tone: .muted)
    }

    public static let max = 99

    /// What the badge prints: `""` (nothing renders) at zero or less, `99+`
    /// past the cap (`labelCases`).
    public static func countBadgeLabel(_ count: Int, max: Int = NavBadges.max) -> String {
        if count <= 0 { return "" }
        return count > max ? "\(max)+" : String(count)
    }

    /// The Chats destination (`chatsCases`): distinct depth-0 chats that need
    /// the viewer or hold an unread reply; an archived root counts only while
    /// it needs the viewer. Attention when one of them needs the viewer.
    public static func chatsBadge(_ groups: [ChatsListGroup], reads: [TreeSessionRead] = []) -> Value {
        let archived = Set(reads.filter { $0.archivedAt != nil }.map(\.sessionId))
        var counted = Set<String>()
        var attention = false
        for group in groups {
            for row in group.rows where row.depth == 0 {
                let lit = row.needsYou || (row.unread && !archived.contains(row.sessionId))
                guard lit else { continue }
                counted.insert(row.sessionId)
                if row.needsYou { attention = true }
            }
        }
        if counted.isEmpty { return .none }
        return Value(count: counted.count, tone: attention ? .attention : .muted)
    }

    /// An agent row, an app row or a chat-list row (`rowCases`): its unread
    /// agent messages, at least 1 while it waits on the viewer.
    public static func rowBadge(unreadCount: Int, needsYou: Bool) -> Value {
        let unread = Swift.max(0, unreadCount)
        if needsYou { return Value(count: Swift.max(1, unread), tone: .attention) }
        return unread > 0 ? Value(count: unread, tone: .muted) : .none
    }

    /// The OS app icon (`appIconCases`): every team's needs-you rows summed.
    public static func appIconBadge(_ teamNeedsYou: [Int]) -> Int {
        teamNeedsYou.reduce(0) { $0 + Swift.max(0, $1) }
    }
}
