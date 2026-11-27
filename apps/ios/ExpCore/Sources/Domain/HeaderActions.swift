import Foundation

/// #EXP-2109 (fixture `header-actions.json`, its `$comment` is the spec; TS
/// `header-actions.ts`): THE header actions' order, the round button's size
/// and the `…` without what the + does.
public enum HeaderActions {
    public enum Action: String, Sendable, CaseIterable {
        case status, primary, call, page, search, inbox, add, more
    }

    public static let circle = 44
    public static let glyph = 20
    public static let gap = 8

    public static func ordered(_ actions: [Action]) -> [Action] {
        Action.allCases.filter(actions.contains)
    }

    static let menuDrops: Set<String> = [
        "open", "open-new-tab", "move", "new", "new-issue", "new-conversation", "newConversation",
    ]

    /// TS `headerMenu`: separators tidied; empty = no button.
    public static func menu(_ entries: [String]) -> [String] {
        let kept = entries.filter { !menuDrops.contains($0) }
        return kept.indices.filter { index in
            kept[index] != "separator" || (index > 0 && index < kept.count - 1 && kept[index - 1] != "separator")
        }.map { kept[$0] }
    }

    /// #EXP-2183 COLLAPSE: the title keeps at least this share of a narrow bar.
    public static let titleMinShare = 0.6
    /// The fold order: the Merge pill first, then the waiting chip.
    public static let collapseOrder: [Action] = [.primary, .status]

    public struct CollapseChip: Sendable, Equatable {
        public let action: Action
        public let full: Double
        public let compact: Double
        public init(action: Action, full: Double, compact: Double) {
            self.action = action
            self.full = full
            self.compact = compact
        }
    }

    /// TS `headerCollapse`: fold the chips in `collapseOrder` until the title
    /// gets `min(title, titleMinShare × bar)`; the folded actions in fold
    /// order. `fixed` = what never folds (back, disc, call, +, …, gaps, padding).
    public static func collapse(bar: Double, title: Double, fixed: Double, chips: [CollapseChip]) -> [Action] {
        let target = min(title, titleMinShare * bar)
        var used = fixed + chips.reduce(0) { $0 + $1.full }
        var folded: [Action] = []
        for action in collapseOrder {
            if bar - used >= target { break }
            guard let chip = chips.first(where: { $0.action == action }) else { continue }
            used -= chip.full - chip.compact
            folded.append(action)
        }
        return folded
    }
}
