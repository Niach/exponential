import Foundation
import XCTest
@testable import ExpCore

// EXP-820: the pool is mirrored BYTE FOR BYTE on web (`lib/chat-suggestions.ts`
// `CHAT_SUGGESTION_POOL`), the desktop (`chat_screen.rs` `CHAT_SUGGESTIONS`)
// and Android; each client's test locks the same list, so a one-sided edit
// fails here.
final class ChatSuggestionsTests: XCTestCase {

    func testPoolIsSixteenDistinctPrompts() {
        XCTAssertEqual(ChatSuggestions.pool.count, 16)
        XCTAssertEqual(Set(ChatSuggestions.pool).count, 16)
        XCTAssertEqual(
            Array(ChatSuggestions.pool.prefix(3)), ["Fix #", "Explain #", "Review #"]
        )
        for suggestion in ChatSuggestions.pool {
            XCTAssertEqual(
                suggestion.trimmingCharacters(in: .whitespaces), suggestion,
                "\(suggestion) carries stray whitespace"
            )
            // A `#` only ever sits where the autocomplete can open on it: at the
            // end, or followed by a space.
            if let hash = suggestion.firstIndex(of: "#") {
                let after = suggestion.index(after: hash)
                XCTAssertTrue(
                    after == suggestion.endIndex || suggestion[after] == " ",
                    "\(suggestion): the # must end the prompt or be followed by a space"
                )
            }
        }
        XCTAssertEqual(ChatSuggestions.count, 4)
    }

    func testDrawsDistinctEntriesInPoolOrder() {
        var seed = 7
        let random: () -> Double = {
            seed = (seed * 48271) % 2147483647
            return Double(seed) / 2147483647
        }
        let picked = ChatSuggestions.pick(random: random)
        XCTAssertEqual(picked.count, 4)
        XCTAssertEqual(Set(picked).count, 4)
        let order = picked.map { ChatSuggestions.pool.firstIndex(of: $0) ?? -1 }
        XCTAssertEqual(order, order.sorted())
        XCTAssertFalse(order.contains(-1))
    }

    func testNeverAsksForMoreThanThePoolHolds() {
        XCTAssertEqual(ChatSuggestions.pick(count: 99, random: { 0 }), ChatSuggestions.pool)
        XCTAssertTrue(ChatSuggestions.pick(count: 0, random: { 0 }).isEmpty)
        // A generator that returns the exclusive upper bound must not index out
        // of the pool (a clamp, not a crash).
        XCTAssertEqual(ChatSuggestions.pick(random: { 1 }).count, 4)
    }

    func testCaretLandsBehindTheIssueRefPlaceholder() {
        XCTAssertEqual(ChatSuggestions.caretOffset("Fix #"), 5)
        XCTAssertEqual(ChatSuggestions.caretOffset("Comment a plan on #"), 19)
        XCTAssertEqual(ChatSuggestions.caretOffset("Split # into sub-issues"), 7)
        XCTAssertNil(ChatSuggestions.caretOffset("Label every issue in the backlog"))
    }
}
