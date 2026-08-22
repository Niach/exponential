import Foundation
import XCTest
import ExpUI

// EXP-551 — the `:shortcode` typeahead half of the emoji contract, mirroring
// the trigger regex and insertion semantics every client implements
// (`packages/emoji/README.md`). Pickers and the typeahead insert UNICODE; the
// markdown bodies stay plain GFM, so nothing here changes serialization.
@MainActor
final class EmojiTokenMatchTests: XCTestCase {
    func testMatchesAnInProgressShortcodeAtTheCaret() {
        let match = IssueEditorModel.emojiMatch(beforeCaret: "Nice :sm")
        XCTAssertEqual(match?.query, "sm")
        XCTAssertEqual(match?.colonOffset, 5)
        XCTAssertEqual(match?.closed, false)
    }

    func testMatchesAtLineStart() {
        let match = IssueEditorModel.emojiMatch(beforeCaret: ":tada")
        XCTAssertEqual(match?.query, "tada")
        XCTAssertEqual(match?.colonOffset, 0)
    }

    func testReportsTheClosingColon() {
        let match = IssueEditorModel.emojiMatch(beforeCaret: "ship it :smile:")
        XCTAssertEqual(match?.query, "smile")
        XCTAssertEqual(match?.colonOffset, 8)
        XCTAssertEqual(match?.closed, true)
    }

    func testAcceptsTheFullShortcodeAlphabet() {
        XCTAssertEqual(IssueEditorModel.emojiMatch(beforeCaret: ":+1")?.query, "+1")
        XCTAssertEqual(IssueEditorModel.emojiMatch(beforeCaret: ":thumbs_up")?.query, "thumbs_up")
        XCTAssertEqual(IssueEditorModel.emojiMatch(beforeCaret: ":e-mail")?.query, "e-mail")
        // Case-insensitive trigger; the lookup lowercases.
        XCTAssertEqual(IssueEditorModel.emojiMatch(beforeCaret: ":SMile")?.query, "SMile")
    }

    func testNeverTriggersOnEverydayColons() {
        // Mid-word colons: a time, a label, a URL scheme.
        XCTAssertNil(IssueEditorModel.emojiMatch(beforeCaret: "12:30"))
        XCTAssertNil(IssueEditorModel.emojiMatch(beforeCaret: "note:"))
        XCTAssertNil(IssueEditorModel.emojiMatch(beforeCaret: "see http://x"))
        XCTAssertNil(IssueEditorModel.emojiMatch(beforeCaret: "https://example.com/a"))
        // Fewer than two shortcode characters, and an ASCII smiley.
        XCTAssertNil(IssueEditorModel.emojiMatch(beforeCaret: ":)"))
        XCTAssertNil(IssueEditorModel.emojiMatch(beforeCaret: ":s"))
        XCTAssertNil(IssueEditorModel.emojiMatch(beforeCaret: ":"))
        // A completed token is no longer in progress once text follows it.
        XCTAssertNil(IssueEditorModel.emojiMatch(beforeCaret: ":smile: ship"))
    }
}

@MainActor
final class EmojiEditorModelTests: XCTestCase {
    private let smile = EmojiRecord(
        unicode: "\u{1F604}", label: "grinning face with smiling eyes",
        group: 0, shortcodes: ["smile"], tags: ["happy"]
    )
    private let thumbsUp = EmojiRecord(
        unicode: "\u{1F44D}", label: "thumbs up", group: 1,
        shortcodes: ["+1", "thumbsup"], tags: ["like"],
        tones: ["\u{1F44D}\u{1F3FB}", "\u{1F44D}\u{1F3FC}", "\u{1F44D}\u{1F3FD}",
                "\u{1F44D}\u{1F3FE}", "\u{1F44D}\u{1F3FF}"]
    )

    private func search(_ query: String) -> [EmojiRecord] {
        let needle = query.lowercased()
        return [smile, thumbsUp].filter { record in
            record.shortcodes.contains { $0.hasPrefix(needle) }
        }
    }

    /// Replays the real callback order the text view produces: seed the caret,
    /// then report the keystroke (only a text change may OPEN a bar, EXP-322).
    private func typed(_ markdown: String, into model: IssueEditorModel) {
        model.load(markdown: markdown, baseURL: nil)
        guard let first = model.blocks.first, case let .text(blockId, content) = first else {
            return XCTFail("expected a leading text block")
        }
        model.updateSelection(blockId: blockId, range: NSRange(location: content.length, length: 0))
        model.updateText(id: blockId, content: content)
    }

    func testOffersCandidatesForAnOpenShortcode() {
        let model = IssueEditorModel()
        model.emojiSearch = { [self] in search($0) }
        typed("Nice :sm", into: model)
        XCTAssertEqual(model.emojiCandidates.map(\.unicode), ["\u{1F604}"])
    }

    func testMenuPickInsertsUnicodePlusASpace() {
        let model = IssueEditorModel()
        model.emojiSearch = { [self] in search($0) }
        typed("Nice :sm", into: model)
        model.applyEmoji(model.emojiCandidates[0])
        // The trailing space is trimmed by the markdown round-trip, but the
        // token (colon + query) is fully replaced — never `:smile:` text.
        XCTAssertEqual(model.currentMarkdown(), "Nice \u{1F604}")
        XCTAssertTrue(model.emojiCandidates.isEmpty)
    }

    func testClosedExactShortcodeAutoCommitsWithoutATrailingSpace() {
        let model = IssueEditorModel()
        model.emojiSearch = { [self] in search($0) }
        typed("Nice :smile:", into: model)
        XCTAssertEqual(model.currentMarkdown(), "Nice \u{1F604}")
        XCTAssertTrue(model.emojiCandidates.isEmpty)
    }

    func testClosedUnknownShortcodeIsLeftAlone() {
        let model = IssueEditorModel()
        model.emojiSearch = { [self] in search($0) }
        typed("Nice :nope:", into: model)
        XCTAssertEqual(model.currentMarkdown(), "Nice :nope:")
        XCTAssertTrue(model.emojiCandidates.isEmpty)
    }

    func testNoTypeaheadWithoutASearchHook() {
        let model = IssueEditorModel()
        typed("Nice :sm", into: model)
        XCTAssertTrue(model.emojiCandidates.isEmpty)
        XCTAssertEqual(model.currentMarkdown(), "Nice :sm")
    }

    // EXP-600: skin tones are gone — an insert is always the BASE unicode,
    // even for records that carry `k` variants.
    func testInsertsTheBaseUnicodeForTonedRecords() {
        let model = IssueEditorModel()
        model.emojiSearch = { [self] in search($0) }
        typed("Ship it :+1", into: model)
        XCTAssertEqual(model.emojiCandidates.map(\.unicode), ["\u{1F44D}"])
        model.applyEmoji(model.emojiCandidates[0])
        XCTAssertEqual(model.currentMarkdown(), "Ship it \u{1F44D}")
    }

    func testRecordsTheBaseUnicodeForRecents() {
        let model = IssueEditorModel()
        model.emojiSearch = { [self] in search($0) }
        var recorded: [String] = []
        model.onEmojiInserted = { recorded.append($0.unicode) }
        typed("Ship it :+1", into: model)
        model.applyEmoji(model.emojiCandidates[0])
        XCTAssertEqual(recorded, ["\u{1F44D}"])
    }
}
