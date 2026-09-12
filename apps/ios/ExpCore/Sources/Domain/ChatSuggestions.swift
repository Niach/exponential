import Foundation

/// EXP-820 — the chips over the Agent page's empty prompt box.
///
/// A POOL rather than three fixed chips: the chat talks to an agent that holds
/// the whole product's MCP surface (issues, boards, labels, automations,
/// sessions on other machines, reviews), so the chips exist to SHOW that range
/// the way the getting-started cards do — each mount draws a few at random.
/// A suggestion ending in `#` lands the caret right after it so the issue-ref
/// autocomplete opens at once; the rest are complete prompts.
///
/// The pool is hand-mirrored BYTE-FOR-BYTE ×4 (web `lib/chat-suggestions.ts`
/// `CHAT_SUGGESTION_POOL`, desktop `chat_screen.rs` `CHAT_SUGGESTIONS`,
/// Android `domain/ChatSuggestions.kt`) and in the same order — change all four
/// together.
public enum ChatSuggestions {
    public static let pool: [String] = [
        "Fix #",
        "Explain #",
        "Review #",
        "Split # into sub-issues",
        "Label every issue in the backlog",
        "Set a priority on every unprioritized issue",
        "Find duplicate issues and link them",
        "Do a code review of the open PRs and file the findings on a new board",
        "Create an automation that labels new issues",
        "Set up a weekly standup digest automation",
        "Draft release notes from the issues completed this month",
        "Summarize what changed across the boards this week",
        "Start a session for # on my other machine",
        "Move stale in-progress issues back to the backlog",
        "Comment a plan on #",
        "Which issues are blocked, and by what?",
    ]

    /// How many chips a mount shows — enough to read as a range, few enough to
    /// stay two short rows on a phone.
    public static let count = 4

    /// `count` DISTINCT suggestions drawn from the pool, in pool order.
    ///
    /// A partial Fisher–Yates over the indices, so the draw is uniform and the
    /// result never repeats a suggestion; `random` returns a value in `0..<1`
    /// (the system generator in the app, a fixed sequence in tests).
    public static func pick(
        count: Int = ChatSuggestions.count,
        random: () -> Double = { Double.random(in: 0..<1) }
    ) -> [String] {
        var indices = Array(pool.indices)
        let take = min(max(count, 0), indices.count)
        for i in 0..<take {
            let span = indices.count - i
            let offset = min(max(Int(random() * Double(span)), 0), span - 1)
            indices.swapAt(i, i + offset)
        }
        return indices.prefix(take).sorted().map { pool[$0] }
    }

    /// EXP-820/827: where the caret lands after a suggestion is inserted — one
    /// past the suggestion's `#` placeholder, wherever it sits, so the issue-ref
    /// autocomplete opens at once. Nil = no placeholder: a complete prompt
    /// leaves the caret at the end. Mirrors web `suggestionCaretOffset`.
    public static func caretOffset(_ suggestion: String) -> Int? {
        guard let hash = suggestion.firstIndex(of: "#") else { return nil }
        return suggestion.distance(from: suggestion.startIndex, to: hash) + 1
    }
}
