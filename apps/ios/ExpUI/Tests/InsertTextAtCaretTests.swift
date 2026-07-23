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
}
