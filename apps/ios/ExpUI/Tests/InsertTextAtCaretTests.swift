import Foundation
import XCTest
import ExpUI

// The composer's `@` affordance (EXP-240): plain-text insertion at the caret
// through the same revision/desiredSelection machinery as applyMention, so the
// inserted trigger immediately arms the autocomplete without breaking the GFM
// byte contract.
@MainActor
final class InsertTextAtCaretTests: XCTestCase {
    func testInsertsAtTheCaretAndArmsTheMentionAutocomplete() {
        let model = IssueEditorModel()
        model.mentionMembers = [MentionMember(name: "Ada Lovelace", email: "ada@example.com")]
        // NB: a TRAILING space would not survive the load round-trip (cmark
        // trims it), so insert mid-string after "Hello ".
        model.load(markdown: "Hello world", baseURL: nil)
        guard let first = model.blocks.first, case let .text(blockId, _) = first else {
            return XCTFail("expected a leading text block")
        }
        model.setFocused(blockId)
        model.updateSelection(blockId: blockId, range: NSRange(location: 6, length: 0))
        model.insertTextAtCaret("@")
        XCTAssertEqual(model.currentMarkdown(), "Hello @world")
        // The fresh `@` is an in-progress mention token at the caret.
        XCTAssertEqual(model.mentionCandidates.map(\.email), ["ada@example.com"])
    }

    func testReplacesTheSelectionRange() {
        let model = IssueEditorModel()
        model.load(markdown: "Hello world", baseURL: nil)
        guard let first = model.blocks.first, case let .text(blockId, _) = first else {
            return XCTFail("expected a leading text block")
        }
        model.setFocused(blockId)
        model.updateSelection(blockId: blockId, range: NSRange(location: 6, length: 5))
        model.insertTextAtCaret("@")
        XCTAssertEqual(model.currentMarkdown(), "Hello @")
    }

    func testAppendsToTheLastTextBlockWithoutACaret() {
        let model = IssueEditorModel()
        model.load(markdown: "Hello", baseURL: nil)
        model.insertTextAtCaret("@")
        XCTAssertEqual(model.currentMarkdown(), "Hello@")
    }

    // EXP-551: the emoji picker inserts UNICODE through this same path, and a
    // toned or ZWJ-joined sequence is several UTF-16 units wide — so the caret
    // it leaves behind has to be measured in UTF-16, not in Characters.
    func testInsertsASkinTonedEmojiAndAdvancesTheCaretByItsUtf16Length() {
        let model = IssueEditorModel()
        model.load(markdown: "Hello world", baseURL: nil)
        guard let first = model.blocks.first, case let .text(blockId, _) = first else {
            return XCTFail("expected a leading text block")
        }
        model.setFocused(blockId)
        model.updateSelection(blockId: blockId, range: NSRange(location: 6, length: 0))
        // Thumbs up, medium skin tone: 4 UTF-16 units, one Character.
        model.insertTextAtCaret("\u{1F44D}\u{1F3FD}")
        XCTAssertEqual(model.currentMarkdown(), "Hello \u{1F44D}\u{1F3FD}world")
        // The follow-up insert lands AFTER the emoji, not inside it.
        model.insertTextAtCaret("!")
        XCTAssertEqual(model.currentMarkdown(), "Hello \u{1F44D}\u{1F3FD}!world")
    }

    func testInsertsAZwjSequenceAndRoundTripsThroughMarkdown() {
        let model = IssueEditorModel()
        model.load(markdown: "Hi", baseURL: nil)
        guard let first = model.blocks.first, case let .text(blockId, content) = first else {
            return XCTFail("expected a leading text block")
        }
        model.setFocused(blockId)
        model.updateSelection(blockId: blockId, range: NSRange(location: content.length, length: 0))
        // Woman technologist: woman + ZWJ + laptop, 5 UTF-16 units.
        let zwj = "\u{1F469}\u{200D}\u{1F4BB}"
        model.insertTextAtCaret(zwj)
        model.insertTextAtCaret("!")
        let markdown = model.currentMarkdown()
        XCTAssertEqual(markdown, "Hi\(zwj)!")
        // Byte-stable across the markdown to blocks round trip — emoji are
        // ordinary GFM text, never escaped or decomposed.
        model.load(markdown: markdown, baseURL: nil)
        XCTAssertEqual(model.currentMarkdown(), markdown)
    }
}
