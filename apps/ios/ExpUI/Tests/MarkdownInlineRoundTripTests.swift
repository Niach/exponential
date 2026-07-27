import Foundation
import XCTest
import ExpUI

// Locks the GFM byte-parity contract for formatting nested INSIDE link text.
// Regression for REV2-19: the load path stacks the link attribute together with
// the emphasis/code attributes of the marks inside it, but the save path picked
// exactly one per attribute run — so `[**bold**](u)` lost its delimiters,
// `` [`code`](u) `` lost the URL outright, and `[**bold** rest](u)` split into
// two adjacent duplicate links. Every save re-derives the whole document, so
// merely opening a web-authored description degraded it for all clients.
final class MarkdownInlineRoundTripTests: XCTestCase {
    private func roundTrip(_ markdown: String) -> String {
        MarkdownConversion.blocksToMarkdown(MarkdownConversion.markdownToBlocks(markdown))
    }

    private func assertStable(_ markdown: String, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(roundTrip(markdown), markdown, file: file, line: line)
    }

    func testBoldInsideLinkSurvives() {
        assertStable("A [**bold**](https://example.com) here")
    }

    func testItalicInsideLinkSurvives() {
        assertStable("A [*it*](https://example.com) here")
    }

    func testStrikethroughInsideLinkSurvives() {
        assertStable("A [~~gone~~](https://example.com) here")
    }

    func testInlineCodeInsideLinkKeepsTheURL() {
        assertStable("A [`code`](https://example.com) here")
    }

    func testPartiallyBoldLinkStaysOneLink() {
        assertStable("A [**bold** rest](https://example.com) here")
    }

    func testBoldItalicInsideLinkSurvives() {
        assertStable("A [***both***](https://example.com) here")
    }

    func testFormattedLinkIsIdempotent() {
        let once = roundTrip("[**b** and `c` and *i*](https://example.com)")
        XCTAssertEqual(roundTrip(once), once)
    }

    // Unformatted links and standalone marks must be untouched by the grouping.
    func testPlainLinkStillRoundTrips() {
        assertStable("A [link](https://example.com) here")
    }

    func testTwoLinksStayDistinct() {
        assertStable("[one](https://a.example) and [two](https://b.example)")
    }

    func testMarksOutsideLinksStillRoundTrip() {
        assertStable("A **bold** and *italic* and `code` mix")
    }
}
