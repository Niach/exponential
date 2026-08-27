import Foundation
import XCTest
import ExpUI

// EXP-655 (Android parity): a tap on the empty band below the last block
// focuses the last TEXT block with the caret at its very end. The revision bump
// is load-bearing — `desiredSelection` is not observed by the view body, so
// without it `updateUIView` would never consume the caret.
@MainActor
final class FocusEndTests: XCTestCase {
    func testFocusesTheLastTextBlockAtItsEnd() {
        let model = IssueEditorModel()
        model.load(markdown: "Hello world", baseURL: nil)
        guard let last = model.blocks.last, case let .text(blockId, content) = last else {
            return XCTFail("expected a trailing text block")
        }
        let revisionBefore = model.revision(for: blockId)
        model.focusEnd()
        XCTAssertEqual(model.focusedBlockId, blockId)
        XCTAssertEqual(model.desiredSelection?.blockId, blockId)
        XCTAssertEqual(model.desiredSelection?.location, content.length)
        XCTAssertGreaterThan(model.revision(for: blockId), revisionBefore)
    }

    /// A description ending in an image already carries a normalized empty text
    /// block after it — the band focuses THAT, and adds no bytes.
    func testFocusesTheNormalizedBlockAfterATrailingImage() {
        let src = "text\n\n![a](https://x/a.png)"
        let model = IssueEditorModel()
        model.load(markdown: src, baseURL: nil)
        let markdownBefore = model.currentMarkdown()
        let blockCountBefore = model.blocks.count
        guard let last = model.blocks.last, case let .text(blockId, content) = last else {
            return XCTFail("expected a normalized trailing text block")
        }
        XCTAssertEqual(content.length, 0)
        model.focusEnd()
        XCTAssertEqual(model.blocks.count, blockCountBefore)
        XCTAssertEqual(model.focusedBlockId, blockId)
        XCTAssertEqual(model.desiredSelection?.location, 0)
        XCTAssertEqual(model.currentMarkdown(), markdownBefore)
    }

    func testAnEmptyModelFocusesItsSoleBlock() {
        let model = IssueEditorModel()
        model.load(markdown: "", baseURL: nil)
        guard let only = model.blocks.first, case let .text(blockId, _) = only else {
            return XCTFail("expected a sole text block")
        }
        model.focusEnd()
        XCTAssertEqual(model.blocks.count, 1)
        XCTAssertEqual(model.focusedBlockId, blockId)
        XCTAssertEqual(model.desiredSelection?.location, 0)
        XCTAssertEqual(model.currentMarkdown(), "")
    }
}
