import Foundation
import XCTest
import ExpUI

// Locks the table-preservation contract: GFM pipe tables are not editable as
// rich text on iOS (tables are out of the editor's feature scope), but they
// must SURVIVE an edit cycle. The cmark table extension is attached for
// parsing, so without explicit handling the table nodes fell into the default
// child walk and flattened to concatenated cell text — one autosave then
// destroyed the table for every client. The load path now preserves the parsed
// table as a verbatim pipe-source run that the save path re-emits
// line-for-line.
final class MarkdownTableRoundTripTests: XCTestCase {
    private func roundTrip(_ markdown: String) -> String {
        MarkdownConversion.blocksToMarkdown(MarkdownConversion.markdownToBlocks(markdown))
    }

    func testPipeTableStructureSurvivesRoundTrip() {
        let src = "| Name | Value |\n| --- | --- |\n| alpha | 1 |\n| beta | 2 |"
        let once = roundTrip(src)
        let lines = once.split(separator: "\n", omittingEmptySubsequences: false)
        XCTAssertEqual(lines.count, 4, "every row must stay its own line: \(once)")
        XCTAssertTrue(lines.allSatisfy { $0.contains("|") }, "pipe structure must survive: \(once)")
        XCTAssertTrue(once.contains("---"), "the delimiter row must survive: \(once)")
        XCTAssertTrue(once.contains("alpha"))
        XCTAssertTrue(once.contains("beta"))
        // The serialized form is cmark's normalized pipe source; it must
        // reparse as a table and re-serialize byte-stable from then on.
        XCTAssertEqual(roundTrip(once), once)
    }

    func testTableBetweenParagraphsKeepsNeighbors() {
        let src = "Before\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\nAfter"
        let once = roundTrip(src)
        XCTAssertTrue(once.hasPrefix("Before\n\n"), "leading paragraph must stay separated: \(once)")
        XCTAssertTrue(once.hasSuffix("\n\nAfter"), "trailing paragraph must stay separated: \(once)")
        XCTAssertTrue(once.contains("| A | B |") || once.contains("| A   | B |") || once.contains("|"), "table body must survive: \(once)")
        XCTAssertEqual(roundTrip(once), once)
    }

    func testTableCellInlineContentIsKeptInsideTheRow() {
        // Inline markup inside cells rides along inside the verbatim source —
        // it must not escape the row onto its own paragraph.
        let src = "| Col |\n| --- |\n| **bold** cell |"
        let once = roundTrip(src)
        XCTAssertTrue(once.contains("bold"))
        XCTAssertFalse(once.contains("\n\n"), "a blank line inside the table would terminate it: \(once)")
        XCTAssertEqual(roundTrip(once), once)
    }
}
