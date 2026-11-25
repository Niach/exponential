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

    /// A parent's badge over the rows it hides (`sumCases`): a folded folder,
    /// the phones' Teams tab. Attention while a counted row needs the viewer.
    public static func sumBadges(_ rows: [Value]) -> Value {
        let lit = rows.filter { $0.count > 0 }
        if lit.isEmpty { return .none }
        return Value(
            count: lit.reduce(0) { $0 + $1.count },
            tone: lit.contains { $0.tone == .attention } ? .attention : .muted
        )
    }

    /// A team row (`teamCases`): its share of Needs you, and while folded its
    /// hidden rows' muted counts (the attention ones are in the share).
    public static func teamBadge(needsYou: Int, open: Bool, rows: [Value]) -> Value {
        let hidden = open ? [] : rows.filter { $0.tone == .muted }
        return sumBadges([Value(count: needsYou, tone: .attention)] + hidden)
    }

    public struct NeedsYouCounts: Equatable, Sendable {
        public let total: Int
        public let teams: [String: Int]
    }

    /// Needs you (`needsYouCases`): `rows` = THE list's rows' teams over every
    /// member team and the Assistant's home; a row outside `teams` (the
    /// Assistant's) and the waiting installs belong to no team.
    public static func needsYouCounts(rows: [String?], teams: [String], installs: Int) -> NeedsYouCounts {
        var counts = Dictionary(teams.map { ($0, 0) }, uniquingKeysWith: { a, _ in a })
        for case let team? in rows where counts[team] != nil { counts[team]! += 1 }
        return NeedsYouCounts(total: rows.count + Swift.max(0, installs), teams: counts)
    }

    /// The OS app icon (`appIconCases`): every account's Needs you total.
    public static func appIconBadge(_ accountNeedsYou: [Int]) -> Int {
        accountNeedsYou.reduce(0) { $0 + Swift.max(0, $1) }
    }

    /// A row's team mark on its lead (`teamMark`), bottom right: the mark's
    /// side, the ground ring around it, how far it hangs past the lead.
    public struct TeamMark: Equatable, Sendable {
        public let mark: Int
        public let ring: Int
        public let offset: Int
    }

    public static let teamMarkSizes: [(lead: Int, mark: TeamMark)] = [
        (40, TeamMark(mark: 20, ring: 2, offset: 2)),
        (24, TeamMark(mark: 14, ring: 2, offset: 3)),
        (0, TeamMark(mark: 10, ring: 1, offset: 4)),
    ]

    public static func teamMark(lead: Int) -> TeamMark {
        (teamMarkSizes.first { $0.lead <= lead } ?? teamMarkSizes[teamMarkSizes.count - 1]).mark
    }
}
