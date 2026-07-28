import Foundation
import XCTest
import ExpCore
import ExpUI

// EXP-327: an image picked through the FILE picker was an attach gesture, not a
// typing one — it belongs after everything already written, whatever the caret
// happens to be doing. `insertImage` keeps splitting at the caret; `appendImage`
// always lands last.
@MainActor
final class AppendImageTests: XCTestCase {
    private func focusedModel(_ markdown: String, caret: Int) -> IssueEditorModel {
        let model = IssueEditorModel()
        model.load(markdown: markdown, baseURL: nil)
        let firstTextId = model.blocks.compactMap { block -> UUID? in
            if case .text(let id, _) = block { return id }
            return nil
        }.first
        if let firstTextId {
            model.setFocused(firstTextId)
            model.updateSelection(blockId: firstTextId, range: NSRange(location: caret, length: 0))
        }
        return model
    }

    private func imageURL(_ model: IssueEditorModel) -> String {
        model.blocks.compactMap { block -> String? in
            if case .image(_, let url, _) = block { return url }
            return nil
        }.first ?? ""
    }

    func testInsertImageSplitsAtTheCaret() {
        let model = focusedModel("before after", caret: 6)
        model.insertImage(data: Data([0x1]), filename: "a.png", contentType: "image/png", width: 4, height: 3)
        let url = imageURL(model)
        XCTAssertEqual(model.currentMarkdown(), "before\n\n![image](\(url))\n\nafter")
    }

    func testAppendImageIgnoresTheCaretAndLandsLast() {
        let model = focusedModel("before after", caret: 6)
        model.appendImage(data: Data([0x1]), filename: "a.png", contentType: "image/png", width: 4, height: 3)
        let url = imageURL(model)
        XCTAssertEqual(model.currentMarkdown(), "before after\n\n![image](\(url))")
    }

    func testAppendImageRegistersThePendingDraft() {
        let model = focusedModel("hello", caret: 5)
        model.appendImage(data: Data([0x1]), filename: "a.png", contentType: "image/png", width: 4, height: 3)
        let url = imageURL(model)
        XCTAssertTrue(MarkdownImageUtils.isDraft(url))
        XCTAssertNotNil(model.pendingImages[url])
    }
}
