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

    /// The latch is TURN-SCOPED. Three editor paths mutate storage themselves,
    /// set `selectedRange` by hand and then call `textViewDidChange` (chip-atom
    /// backspace, list continuation, list exit) — none of them produces the
    /// selection callback that consumes the latch. Left armed, it re-opens the
    /// bar on the next plain tap inside an existing `#EXP-238`, which is the
    /// exact bug EXP-322 set out to fix (EXP-322).
    func testAnEditWithNoSettledCaretCallbackDoesNotArmTheNextTap() async throws {
        let (model, id, content) = self.model(markdown: "Fixes #EXP-42 today")
        model.updateSelection(blockId: id, range: NSRange(location: 19, length: 0))
        // An edit whose caret lands where no token is being typed: the bar does
        // not open now...
        model.updateText(id: id, content: content)
        XCTAssertTrue(model.issueRefCandidates.isEmpty)
        try await Task.sleep(nanoseconds: 50_000_000)
        // ...and a later pure tap inside the existing token must not open it
        // either.
        model.updateSelection(blockId: id, range: NSRange(location: 12, length: 0))
        XCTAssertTrue(model.issueRefCandidates.isEmpty)
    }

    /// The latch still survives long enough for the settled-caret callback that
    /// UIKit delivers in the SAME runloop turn as the text change.
    func testTheSameTurnSelectionCallbackStillOpensTheBar() {
        let (model, id, content) = self.model(markdown: "Fixes #EX")
        model.updateText(id: id, content: content)
        model.updateSelection(blockId: id, range: NSRange(location: 9, length: 0))
        XCTAssertEqual(model.issueRefCandidates.map(\.identifier), ["EXP-42"])
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

    /// ...but a pass that INSERTS a title attachment shifts every offset after
    /// it, and the model's own selection callback is suppressed while the text
    /// view applies it. A stale `@`-token offset would make the next candidate
    /// tap replace one character too few — eating the character before the `@`
    /// and saving the leftover query. Close the bar instead (EXP-322).
    func testADecorationPassThatShiftsOffsetsClosesTheBar() {
        let (model, id, content) = self.model(markdown: "Fixes #EXP-42 ping @ad")
        model.issueRefResolver = { $0 == "EXP-42" ? "issue-id" : nil }
        model.issueRefTitleResolver = { _ in "Fix login flow" }
        model.updateSelection(blockId: id, range: NSRange(location: content.length, length: 0))
        model.updateText(id: id, content: content)
        XCTAssertEqual(model.mentionCandidates.map(\.email), ["ada@example.com"])

        let caret = NSRange(location: content.length, length: 0)
        let decorated = model.chipDecoration(for: content, selection: caret)
        XCTAssertTrue(decorated.changed)
        model.applyDecoration(
            id: id,
            content: decorated.attributed,
            selection: decorated.selection,
            lengthDelta: decorated.attributed.length - content.length
        )
        XCTAssertTrue(model.mentionCandidates.isEmpty)
    }
}
