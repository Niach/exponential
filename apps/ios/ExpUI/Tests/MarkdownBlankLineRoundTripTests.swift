import Foundation
import XCTest
import ExpUI

// EXP-689: intentional blank lines must survive save + reload. GFM folds bare
// blank-line runs into a single block boundary on every parser, so the
// contract stores each INTERIOR empty paragraph as an `&nbsp;` line (web's
// MarkdownParagraph, EXP-7). Byte-locked on all four clients.
final class MarkdownBlankLineRoundTripTests: XCTestCase {
    private func roundTrip(_ markdown: String) -> String {
        MarkdownConversion.blocksToMarkdown(MarkdownConversion.markdownToBlocks(markdown))
    }

    private func assertStable(_ markdown: String, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(roundTrip(markdown), markdown, file: file, line: line)
    }

    func testBlankLineBetweenParagraphsRoundTrips() {
        assertStable("First\n\n&nbsp;\n\nSecond")
    }

    func testTwoBlankLinesBetweenParagraphsRoundTrip() {
        assertStable("First\n\n&nbsp;\n\n&nbsp;\n\nSecond")
    }

    func testBlankLineParsesToAnEmptyEditorLine() {
        // No invisible U+00A0 in the text view: the marker folds to an empty line.
        let blocks = MarkdownConversion.markdownToBlocks("First\n\n&nbsp;\n\nSecond")
        XCTAssertEqual(blocks.count, 1)
        guard case .text(_, let content) = blocks[0] else { return XCTFail("expected a text block") }
        XCTAssertEqual(content.string, "First\n\nSecond")
    }

    func testEditorTypedBlankLineIsWrittenAsTheMarker() {
        // Two Returns in the text view = an empty line inside the block.
        let typed = NSAttributedString(string: "First\n\nSecond")
        XCTAssertEqual(MarkdownConversion.attributedStringToMarkdown(typed), "First\n\n&nbsp;\n\nSecond")
    }

    func testLeadingAndTrailingBlankLinesAreDropped() {
        let typed = NSAttributedString(string: "\n\nOnly line\n\n")
        XCTAssertEqual(MarkdownConversion.attributedStringToMarkdown(typed), "Only line")
        XCTAssertEqual(roundTrip("&nbsp;\n\nOnly line\n\n&nbsp;"), "Only line")
    }

    func testLiteralNoBreakSpaceParagraphConvergesToTheMarker() {
        XCTAssertEqual(roundTrip("First\n\n\u{00A0}\n\nSecond"), "First\n\n&nbsp;\n\nSecond")
    }

    func testBlankLinesInsideAFenceStayCode() {
        assertStable("```\na\n\nb\n```")
    }

    func testOrdinaryParagraphSpacingIsUntouched() {
        assertStable("First paragraph.\n\nSecond paragraph.")
        assertStable("- one\n- two\n- three")
        assertStable("# Title\n\nSome body text")
    }
}
