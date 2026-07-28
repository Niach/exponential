import Foundation
import XCTest
import ExpUI

/// EXP-322: the `@`/`#` candidate bar opens only on a DOCUMENT change, mirroring
/// web's `docChanged` rule in `apps/web/src/lib/editor-autocomplete.ts`. The
/// reported bug was a bar that popped whenever the caret merely landed inside an
/// existing `#EXP-238` and then would not go away.
@MainActor
final class AutocompleteArmingTests: XCTestCase {

    private func model(markdown: String) -> (IssueEditorModel, UUID, NSAttributedString) {
        let model = IssueEditorModel()
        model.mentionMembers = [MentionMember(name: "Ada Lovelace", email: "ada@example.com")]
        model.issueRefSearch = { _ in [IssueRefCandidate(identifier: "EXP-42", title: "Fix login flow")] }
        model.load(markdown: markdown, baseURL: nil)
        guard case let .text(id, content) = model.blocks[0] else {
            fatalError("expected a leading text block")
        }
        return (model, id, content)
    }

    // MARK: - The regression

    func testTappingInsideAnExistingIssueRefDoesNotOpenTheBar() {
        let (model, id, _) = self.model(markdown: "Fixes #EXP-42 today")
        // Caret between "#EXP-4" and "2" — exactly the bug screenshot.
        model.updateSelection(blockId: id, range: NSRange(location: 12, length: 0))
        XCTAssertTrue(model.issueRefCandidates.isEmpty)
    }

    func testTappingInsideAnExistingMentionDoesNotOpenTheBar() {
        let (model, id, _) = self.model(markdown: "Ping @ada@example.com now")
        model.updateSelection(blockId: id, range: NSRange(location: 12, length: 0))
        XCTAssertTrue(model.mentionCandidates.isEmpty)
    }

    // MARK: - Opening and tracking

    func testATextChangeOpensTheBar() {
        let (model, id, content) = self.model(markdown: "Fixes #EX")
        model.updateSelection(blockId: id, range: NSRange(location: 9, length: 0))
        model.updateText(id: id, content: content)
        XCTAssertEqual(model.issueRefCandidates.map(\.identifier), ["EXP-42"])
    }

    func testAnOpenBarSurvivesAnUnarmedSelectionAtTheSameCaret() {
        let (model, id, content) = self.model(markdown: "Fixes #EX")
        model.updateSelection(blockId: id, range: NSRange(location: 9, length: 0))
        model.updateText(id: id, content: content)
        XCTAssertFalse(model.issueRefCandidates.isEmpty)
        // The settled-caret callback that follows every keystroke.
        model.updateSelection(blockId: id, range: NSRange(location: 9, length: 0))
        XCTAssertFalse(model.issueRefCandidates.isEmpty)
        // ...and a second, unarmed one (a pure tap) still keeps it open, because
        // the token is still under the caret.
        model.updateSelection(blockId: id, range: NSRange(location: 9, length: 0))
        XCTAssertFalse(model.issueRefCandidates.isEmpty)
    }

    func testMovingTheCaretOffTheTokenClosesTheBar() {
        let (model, id, content) = self.model(markdown: "Fixes #EX now")
        model.updateSelection(blockId: id, range: NSRange(location: 9, length: 0))
        model.updateText(id: id, content: content)
        XCTAssertFalse(model.issueRefCandidates.isEmpty)
        model.updateSelection(blockId: id, range: NSRange(location: 2, length: 0))
        XCTAssertTrue(model.issueRefCandidates.isEmpty)
    }

    /// The `@`/`#` buttons in the toolbar and the comment composer insert a
    /// trigger character, which counts as a text change. (The document starts
    /// empty because a trailing space does not survive the markdown round
    /// trip, and a trigger glued to a word is deliberately not a trigger.)
    func testTheToolbarAffordanceOpensTheBar() {
        let (model, id, _) = self.model(markdown: "")
        model.updateSelection(blockId: id, range: NSRange(location: 0, length: 0))
        model.insertTextAtCaret("#")
        XCTAssertFalse(model.issueRefCandidates.isEmpty)

        let (mentionModel, mentionId, _) = self.model(markdown: "")
        mentionModel.updateSelection(blockId: mentionId, range: NSRange(location: 0, length: 0))
        mentionModel.insertTextAtCaret("@")
        XCTAssertFalse(mentionModel.mentionCandidates.isEmpty)
    }

    // MARK: - Paths that must NOT arm

    func testApplyingACandidateDoesNotReopenTheBar() {
        let (model, id, content) = self.model(markdown: "Fixes #EX")
        model.issueRefResolver = { _ in "issue-id" }
        model.updateSelection(blockId: id, range: NSRange(location: 9, length: 0))
        model.updateText(id: id, content: content)
        model.applyIssueRef(model.issueRefCandidates[0])
        XCTAssertEqual(model.currentMarkdown(), "Fixes #EXP-42")
        // The caret move the text view reports after re-applying the content.
        guard case let .text(_, updated) = model.blocks[0] else { return XCTFail("expected a text block") }
        model.updateSelection(blockId: id, range: NSRange(location: updated.length, length: 0))
        XCTAssertTrue(model.issueRefCandidates.isEmpty)
    }

    /// A decoration pass is not an edit — if it armed, the NEXT pure tap would
    /// pop the bar.
    func testADecorationPassDoesNotArmTheBar() {
        let (model, id, content) = self.model(markdown: "Fixes #EXP-42 today")
        model.issueRefResolver = { $0 == "EXP-42" ? "issue-id" : nil }
        model.updateText(id: id, content: content)
        model.updateSelection(blockId: id, range: NSRange(location: 0, length: 0))
        let decorated = model.chipDecoration(for: content, selection: NSRange(location: 0, length: 0))
        model.applyDecoration(id: id, content: decorated.attributed)
        model.updateSelection(blockId: id, range: NSRange(location: 12, length: 0))
        XCTAssertTrue(model.issueRefCandidates.isEmpty)
    }
}
