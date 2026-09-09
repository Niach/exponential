import Foundation
import XCTest
import ExpUI

// EXP-802: the steer composer drives a one-text-block `IssueEditorModel` so
// `@`/`#`/`:` can splice at the caret — but what it SENDS is chat prose, not a
// stored document. These lock the two halves of that: a draft reads back
// exactly as typed (`plainText`, never the markdown serializer's round trip),
// and an external write (a `/` command, an `[Image #k]` marker) may not drop
// the keyboard (`setPlainText`, never `load`).
@MainActor
final class PlainTextDraftTests: XCTestCase {
    private func focusedDraft() -> (IssueEditorModel, UUID) {
        let model = IssueEditorModel()
        let id = model.blocks.compactMap { block -> UUID? in
            if case .text(let id, _) = block { return id }
            return nil
        }.first!
        model.setFocused(id)
        return (model, id)
    }

    /// The headline case: markdown's own punctuation is just punctuation in a
    /// message to an agent.
    func testMarkdownPunctuationSurvivesTheDraftRoundTrip() {
        for draft in [
            "a_b_c",
            "rename fetch_all_rows to fetchAll",
            "why does *this* break",
            "# not a heading",
            "1. not a list",
            "run `git status` | head",
            "path\\to\\file",
        ] {
            let (model, _) = focusedDraft()
            model.setPlainText(draft)
            XCTAssertEqual(model.plainText, draft, "draft mangled: \(draft)")
        }
    }

    /// The concrete reason `plainText` exists rather than `currentMarkdown()`:
    /// the markdown writer produces a DOCUMENT, and a blank line in one is the
    /// contract's `&nbsp;` paragraph (EXP-689) — not what was typed.
    func testABlankLineSurvivesTheDraftButNotTheMarkdownWriter() {
        let (model, _) = focusedDraft()
        model.setPlainText("first\n\nsecond")
        XCTAssertEqual(model.plainText, "first\n\nsecond")
        XCTAssertNotEqual(model.currentMarkdown(), "first\n\nsecond")
    }

    /// The same guarantee for text the USER typed rather than one written in.
    func testTypedTextReadsBackVerbatim() {
        let (model, id) = focusedDraft()
        model.updateText(id: id, content: NSAttributedString(string: "a_b_c"))
        XCTAssertEqual(model.plainText, "a_b_c")
    }

    /// `load()` clears focus (a document load has no caret); the setter the
    /// composer uses must not, or the keyboard drops mid-message.
    func testSetPlainTextKeepsFocusAndPutsTheCaretAtTheEnd() {
        let (model, id) = focusedDraft()
        model.updateText(id: id, content: NSAttributedString(string: "/cl"))
        model.setPlainText("/clear ")
        XCTAssertEqual(model.focusedBlockId, id)
        XCTAssertTrue(model.isEditing)
        XCTAssertEqual(model.consumeDesiredSelection(for: id), 7)
    }

    /// Writing back the text the model already holds may not re-apply it: the
    /// text view would take the content again and yank the caret to the end.
    func testWritingTheSameTextIsANoOp() {
        let (model, id) = focusedDraft()
        model.setPlainText("hello")
        _ = model.consumeDesiredSelection(for: id)
        let revision = model.revision(for: id)
        model.setPlainText("hello")
        XCTAssertEqual(model.revision(for: id), revision)
        XCTAssertNil(model.consumeDesiredSelection(for: id))
    }

    /// A resolved `#EXP-1` chip carries its title on one attachment character
    /// (see `issueRefTitleResolver`). It was never typed, so it never goes out.
    func testResolvedIssueRefChipTitleNeverReachesTheDraft() {
        let (model, _) = focusedDraft()
        model.issueRefResolver = { $0 == "EXP-1" ? "issue-1" : nil }
        model.issueRefTitleResolver = { $0 == "EXP-1" ? "A title nobody typed" : nil }
        model.setPlainText("look at #EXP-1 please")
        XCTAssertEqual(model.plainText, "look at #EXP-1 please")
    }
}
