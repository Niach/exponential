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
}
