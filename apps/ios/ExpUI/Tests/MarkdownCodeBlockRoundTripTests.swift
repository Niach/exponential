import Foundation
import XCTest
import ExpUI

// Locks the GFM byte-parity contract for fenced-code round-trips. Before the
// fix: interior/leading blank lines inside a fence were deleted on every save
// (the split dropped zero-length lines), a blank-only fence evaporated at load,
// and back-to-back fences with different languages merged into one fence with
// the first language — each corruption then synced to every client.
final class MarkdownCodeBlockRoundTripTests: XCTestCase {
    private func roundTrip(_ markdown: String) -> String {
        MarkdownConversion.blocksToMarkdown(MarkdownConversion.markdownToBlocks(markdown))
    }

    func testInteriorBlankLineInFenceSurvives() {
        let src = "```\nlet a = 1\n\nlet b = 2\n```"
        XCTAssertEqual(roundTrip(src), src)
        XCTAssertEqual(roundTrip(roundTrip(src)), src) // idempotent
    }

    func testMultipleConsecutiveBlankLinesInsideFenceSurvive() {
        let src = "```\na\n\n\n\nb\n```"
        XCTAssertEqual(roundTrip(src), src)
    }

    func testBlankLineAtFenceStartSurvives() {
        let src = "```\n\na\n```"
        XCTAssertEqual(roundTrip(src), src)
    }

    func testBlankLineAtFenceEndIsLossyButStable() {
        // The trailing blank line has no zero-length line of its own on save (it
        // is the last content line's terminator), so it is dropped — an accepted
        // lossy edge that is stable from the second pass on.
        XCTAssertEqual(roundTrip("```\na\n\n```"), "```\na\n```")
        XCTAssertEqual(roundTrip("```\na\n```"), "```\na\n```") // idempotent
    }

    func testBlankLineOnlyFenceSurvives() {
        // Locks the load-side edits: without them the "\n" literal becomes an
        // empty attributed run and the whole fence vanishes.
        let src = "```\n\n```"
        XCTAssertEqual(roundTrip(src), src)
    }

    func testAdjacentFencesWithDifferentLanguagesStaySeparate() {
        let src = "```swift\nlet a = 1\n```\n\n```python\nb = 2\n```"
        XCTAssertEqual(roundTrip(src), src)
        XCTAssertEqual(roundTrip(roundTrip(src)), src) // idempotent
    }

    func testUntaggedFenceAfterTaggedFenceClosesTheFirst() {
        // nil-vs-"swift" language comparison must still close the first fence.
        let src = "```swift\na\n```\n\n```\nb\n```"
        XCTAssertEqual(roundTrip(src), src)
    }

    func testAdjacentUntaggedFencesMergeStably() {
        // Same-language (both untagged) adjacent fences merge — accepted; the
        // merged form is then stable.
        XCTAssertEqual(roundTrip("```\na\n```\n\n```\nb\n```"), "```\na\nb\n```")
        XCTAssertEqual(roundTrip("```\na\nb\n```"), "```\na\nb\n```") // idempotent
    }

    func testFenceFollowedByParagraphRoundTripsWithoutExtraBlankLine() {
        // A fence closed WITH a trailing newline plus the generic block separator
        // used to accrete an extra blank line per save; closing without the
        // newline keeps it byte-stable.
        let src = "```\na\n```\n\npara"
        XCTAssertEqual(roundTrip(src), src)
        XCTAssertEqual(roundTrip(roundTrip(src)), src) // idempotent
    }
}

// The editor-model half of the guarantee: loading a fenced-code document must
// baseline lastSavedMarkdown to the ORIGINAL bytes, so the ~1.2s autosave never
// rewrites a document the user never touched.
@MainActor
final class FencedCodeEditorModelTests: XCTestCase {
    func testLoadedFenceDocumentIsNotDirtyAndSerializesUnchanged() {
        let model = IssueEditorModel()
        let src = "```swift\nlet a = 1\n\nlet b = 2\n```\n\n```python\nprint(1)\n```"
        model.load(markdown: src, baseURL: nil)
        XCTAssertEqual(model.currentMarkdown(), src)
        XCTAssertFalse(model.isDirty)
    }
}
