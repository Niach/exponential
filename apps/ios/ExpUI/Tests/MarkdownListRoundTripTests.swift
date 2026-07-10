import Foundation
import XCTest
import ExpUI

// Locks the GFM byte-parity contract for list round-trips. Regression for
// REV-10: the ordered-list "<n>. " prefix is baked into the attributed string
// as literal text at load, but the strip regex in extractInlineMarkdown only
// covered the bullet/checkbox glyphs — so every save re-emitted the index on
// top of the unstripped prefix ("1. First" → "1. 1. First"), compounding on
// each iOS edit cycle and syncing the corruption to all clients.
final class MarkdownListRoundTripTests: XCTestCase {
    private func roundTrip(_ markdown: String) -> String {
        MarkdownConversion.blocksToMarkdown(MarkdownConversion.markdownToBlocks(markdown))
    }

    func testOrderedListSurvivesRoundTripUnchanged() {
        let src = "1. First\n2. Second\n3. Third"
        XCTAssertEqual(roundTrip(src), src)
    }

    func testOrderedListIsIdempotentAcrossASecondPass() {
        let once = roundTrip("1. First\n2. Second")
        XCTAssertEqual(roundTrip(once), once)
    }

    func testOrderedListWithCustomStartKeepsIndices() {
        // Load reads cmark_node_get_list_start; save re-emits
        // .markdownListItemIndex — a non-1 start must survive.
        let src = "3. Third\n4. Fourth"
        XCTAssertEqual(roundTrip(src), src)
    }

    func testBulletAndTaskListsStillRoundTrip() {
        // Guards the glyph half of the strip regex against regressions from
        // the ordered-form alternation.
        XCTAssertEqual(roundTrip("- Alpha\n- Beta"), "- Alpha\n- Beta")
        XCTAssertEqual(roundTrip("- [ ] Open\n- [x] Done"), "- [ ] Open\n- [x] Done")
    }
}

// The editor-model half of the failure scenario: loading an ordered list must
// baseline lastSavedMarkdown to the ORIGINAL bytes (not a duplicated form), so
// the 1.2s autosave never fires without a real user edit.
@MainActor
final class OrderedListEditorModelTests: XCTestCase {
    func testLoadedOrderedListIsNotDirtyAndSerializesUnchanged() {
        let model = IssueEditorModel()
        let src = "1. First\n2. Second"
        model.load(markdown: src, baseURL: nil)
        XCTAssertEqual(model.currentMarkdown(), src)
        XCTAssertFalse(model.isDirty)
    }
}
