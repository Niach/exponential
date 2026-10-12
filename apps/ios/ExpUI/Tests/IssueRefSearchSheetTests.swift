import Foundation
import XCTest
import ExpUI

/// SLOP-28 (EXP-1259 D4): with `issueRefOpensSheet` the `#` typeahead is THE
/// search list in a bottom sheet — a TYPED token opens it (prefilled with the
/// token), a caret move never does, a pick writes `#IDENT `, and Backspace in
/// its empty input deletes the `#`. The arming rules are EXP-322's
/// (`AutocompleteArmingTests`); only the presentation changed.
@MainActor
final class IssueRefSearchSheetTests: XCTestCase {
    private func model(markdown: String) -> (IssueEditorModel, UUID, NSAttributedString) {
        let model = IssueEditorModel()
        model.issueRefOpensSheet = true
        model.issueRefSearch = { _ in [IssueRefCandidate(identifier: "EXP-42", title: "Fix login flow")] }
        model.load(markdown: markdown, baseURL: nil)
        guard case let .text(id, content) = model.blocks[0] else {
            fatalError("expected a leading text block")
        }
        return (model, id, content)
    }

    func testATypedTokenOpensTheSheetPrefilledAndNoInlineBar() {
        let (model, id, content) = model(markdown: "Explain #sid")
        model.updateSelection(blockId: id, range: NSRange(location: 12, length: 0))
        model.updateText(id: id, content: content)
        XCTAssertEqual(model.issueRefSearchRequest?.query, "sid")
        XCTAssertTrue(model.issueRefCandidates.isEmpty)
        XCTAssertFalse(model.showsAutocompleteMenu)
    }

    func testTheToolbarHashOpensAnEmptySheet() {
        let (model, id, _) = model(markdown: "")
        model.updateSelection(blockId: id, range: NSRange(location: 0, length: 0))
        model.insertTextAtCaret("#")
        XCTAssertEqual(model.issueRefSearchRequest?.query, "")
    }

    func testACaretMoveIntoAnExistingRefNeverOpensIt() {
        let (model, id, _) = model(markdown: "Fixes #EXP-42 today")
        model.updateSelection(blockId: id, range: NSRange(location: 12, length: 0))
        XCTAssertNil(model.issueRefSearchRequest)
    }

    func testAPickWritesThePlainRefAndClosesTheSheet() {
        let (model, id, content) = model(markdown: "Fixes #EX")
        model.updateSelection(blockId: id, range: NSRange(location: 9, length: 0))
        model.updateText(id: id, content: content)
        model.completeIssueRefSearch(IssueRefCandidate(identifier: "EXP-42", title: "Fix login flow"))
        XCTAssertNil(model.issueRefSearchRequest)
        XCTAssertEqual(model.currentMarkdown(), "Fixes #EXP-42")
        XCTAssertEqual(model.focusedBlockId, id)
    }

    func testBackspaceInTheEmptyInputDeletesTheHash() {
        let (model, id, content) = model(markdown: "Explain #")
        model.updateSelection(blockId: id, range: NSRange(location: 9, length: 0))
        model.updateText(id: id, content: content)
        XCTAssertNotNil(model.issueRefSearchRequest)
        model.dismissIssueRefSearch(deletingHash: true)
        XCTAssertNil(model.issueRefSearchRequest)
        XCTAssertEqual(model.currentMarkdown(), "Explain")
    }

    func testASwipeDownKeepsTheTextAsTyped() {
        let (model, id, content) = model(markdown: "see (#side")
        model.updateSelection(blockId: id, range: NSRange(location: 10, length: 0))
        model.updateText(id: id, content: content)
        XCTAssertEqual(model.issueRefSearchRequest?.query, "side")
        model.dismissIssueRefSearch()
        XCTAssertNil(model.issueRefSearchRequest)
        XCTAssertEqual(model.currentMarkdown(), "see (#side")
    }
}
