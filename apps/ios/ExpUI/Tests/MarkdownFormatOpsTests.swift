import Foundation
import UIKit
import XCTest
import ExpUI

// EXP-568 — the formatting rail's ops are pure attributed-string edits, so the
// contract they have to hold is the GFM one: apply an op, serialize, and the
// markdown every other client reads must be exactly what the button promised.
// Everything below therefore round-trips through MarkdownConversion rather
// than asserting on attribute keys.
final class MarkdownFormatOpsTests: XCTestCase {
    private func content(_ markdown: String) -> NSMutableAttributedString {
        let blocks = MarkdownConversion.markdownToBlocks(markdown)
        for block in blocks {
            if case .text(_, let attributed) = block, attributed.length > 0 {
                return NSMutableAttributedString(attributedString: attributed)
            }
        }
        return NSMutableAttributedString(string: "", attributes: MarkdownStyle.baseAttributes)
    }

    private func serialize(_ text: NSAttributedString) -> String {
        MarkdownConversion.blocksToMarkdown([.text(id: UUID(), attributedContent: text)])
    }

    private func whole(_ text: NSAttributedString) -> NSRange {
        NSRange(location: 0, length: text.length)
    }

    // MARK: - Bold / italic / strikethrough

    func testToggleBoldWrapsTheSelection() {
        let text = content("Hello")
        MarkdownFormatOps.toggleBold(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "**Hello**")
    }

    func testToggleBoldTwiceRestoresPlainText() {
        let text = content("Hello")
        MarkdownFormatOps.toggleBold(in: text, range: whole(text))
        MarkdownFormatOps.toggleBold(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "Hello")
    }

    func testToggleItalicWrapsTheSelection() {
        let text = content("Hello")
        MarkdownFormatOps.toggleItalic(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "*Hello*")
    }

    func testBoldAndItalicCompose() {
        let text = content("Hello")
        MarkdownFormatOps.toggleBold(in: text, range: whole(text))
        MarkdownFormatOps.toggleItalic(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "***Hello***")
    }

    func testToggleStrikethroughRoundTrips() {
        let text = content("Hello")
        MarkdownFormatOps.toggleStrikethrough(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "~~Hello~~")
        MarkdownFormatOps.toggleStrikethrough(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "Hello")
    }

    // A code span is exclusive of the emphasis delimiters on every client, so
    // bolding across one must step over it instead of emitting `**`code`**`.
    func testBoldSkipsInlineCodeRuns() {
        let text = content("a`code`b")
        MarkdownFormatOps.toggleBold(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "**a**`code`**b**")
    }

    // `**hi **` is not bold in GFM — the closing delimiter is not left-flanking
    // — so a selection that swept up trailing whitespace must not include it.
    func testBoldTrimsTrailingWhitespaceOutOfTheMark() {
        let text = NSMutableAttributedString(string: "hi ", attributes: MarkdownStyle.baseAttributes)
        MarkdownFormatOps.toggleBold(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "**hi**")
    }

    // The serializer drops bold inside headings, so the button must be a no-op
    // there rather than silently flattening the heading font.
    func testBoldLeavesHeadingsAlone() {
        let text = content("## Title")
        MarkdownFormatOps.toggleBold(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "## Title")
    }

    func testIsBoldReportsTheAppliedMark() {
        let text = content("Hello")
        XCTAssertFalse(MarkdownFormatOps.isBold(text, range: whole(text)))
        MarkdownFormatOps.toggleBold(in: text, range: whole(text))
        XCTAssertTrue(MarkdownFormatOps.isBold(text, range: whole(text)))
    }

    // MARK: - Typing attributes (collapsed caret)

    func testTypingAttributeBoldTogglesBothWays() {
        var attrs = MarkdownStyle.baseAttributes
        attrs = MarkdownFormatOps.togglingBold(attrs)
        XCTAssertTrue(expFontHasBold(attrs[.font] as? UIFont))
        attrs = MarkdownFormatOps.togglingBold(attrs)
        XCTAssertFalse(expFontHasBold(attrs[.font] as? UIFont))
    }

    func testTypingAttributeStrikethroughTogglesBothWays() {
        var attrs = MarkdownStyle.baseAttributes
        attrs = MarkdownFormatOps.togglingStrikethrough(attrs)
        XCTAssertEqual(attrs[.markdownStrikethrough] as? Bool, true)
        attrs = MarkdownFormatOps.togglingStrikethrough(attrs)
        XCTAssertNil(attrs[.markdownStrikethrough])
    }

    func testHeadingTypingAttributesCarryTheLevelAndDropListState() {
        var attrs = MarkdownStyle.baseAttributes
        attrs[.markdownListType] = "bullet"
        let heading = MarkdownFormatOps.headingTypingAttributes(level: 2, from: attrs)
        XCTAssertEqual(heading[.markdownHeadingLevel] as? Int, 2)
        XCTAssertNil(heading[.markdownListType])
        let body = MarkdownFormatOps.headingTypingAttributes(level: 0, from: heading)
        XCTAssertNil(body[.markdownHeadingLevel])
    }

    // MARK: - Headings

    func testSetHeadingPromotesAndRevertsAParagraph() {
        let text = content("Hello")
        MarkdownFormatOps.setHeading(level: 2, in: text, paragraphRange: whole(text))
        XCTAssertEqual(serialize(text), "## Hello")
        MarkdownFormatOps.setHeading(level: 0, in: text, paragraphRange: whole(text))
        XCTAssertEqual(serialize(text), "Hello")
    }

    // Line formats are orthogonal attribute keys: without the strip, a heading
    // applied to a quote serializes as "> # quoted".
    func testSetHeadingStripsBlockquote() {
        let text = content("> quoted")
        MarkdownFormatOps.setHeading(level: 1, in: text, paragraphRange: whole(text))
        XCTAssertEqual(serialize(text), "# quoted")
    }

    // The list prefix is baked into the TEXT, so it has to be deleted too.
    func testSetHeadingStripsTheBakedListPrefix() {
        let text = content("- item")
        MarkdownFormatOps.setHeading(level: 1, in: text, paragraphRange: whole(text))
        XCTAssertEqual(serialize(text), "# item")
    }

    func testSetHeadingStripsCodeBlocks() {
        let text = content("```\ncode\n```")
        MarkdownFormatOps.setHeading(level: 3, in: text, paragraphRange: whole(text))
        XCTAssertEqual(serialize(text), "### code")
    }

    func testHeadingLevelQueryReadsTheParagraph() {
        let text = content("### Title")
        XCTAssertEqual(MarkdownFormatOps.headingLevel(text, at: 0), 3)
        MarkdownFormatOps.setHeading(level: 0, in: text, paragraphRange: whole(text))
        XCTAssertEqual(MarkdownFormatOps.headingLevel(text, at: 0), 0)
    }

    // MARK: - Clear formatting

    func testClearFormattingStripsEveryInlineMark() {
        let text = content("**b** and *i* and `c` and [l](https://example.com) and ~~s~~")
        MarkdownFormatOps.clearFormatting(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "b and i and c and l and s")
    }

    func testClearFormattingDropsTheHeading() {
        let text = content("# Title")
        MarkdownFormatOps.clearFormatting(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "Title")
    }

    func testClearFormattingDropsListAndQuote() {
        let list = content("- item")
        MarkdownFormatOps.clearFormatting(in: list, range: whole(list))
        XCTAssertEqual(serialize(list), "item")

        let quote = content("> quoted")
        MarkdownFormatOps.clearFormatting(in: quote, range: whole(quote))
        XCTAssertEqual(serialize(quote), "quoted")
    }

    func testClearInlineFormattingKeepsTheHeading() {
        let text = content("# **Title**")
        MarkdownFormatOps.clearInlineFormatting(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "# Title")
    }

    func testClearedTypingAttributesAreAPlainParagraph() {
        let attrs = MarkdownFormatOps.clearedTypingAttributes()
        XCTAssertNil(attrs[.markdownHeadingLevel])
        XCTAssertNil(attrs[.markdownListType])
        XCTAssertNil(attrs[.markdownBlockquote])
        XCTAssertFalse(expFontHasBold(attrs[.font] as? UIFont))
    }

    // MARK: - Links

    func testApplyLinkOverASelection() {
        let text = content("Hello")
        MarkdownFormatOps.applyLink("https://example.com", in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "[Hello](https://example.com)")
    }

    func testApplyLinkAtACollapsedCaretInsertsTheURL() {
        let text = NSMutableAttributedString(string: "", attributes: MarkdownStyle.baseAttributes)
        let applied = MarkdownFormatOps.applyLink(
            "https://x.example", in: text, range: NSRange(location: 0, length: 0))
        XCTAssertEqual(applied.length, 17)
        XCTAssertEqual(serialize(text), "[https://x.example](https://x.example)")
    }

    func testRemoveLinkLeavesThePlainText() {
        let text = content("[Hello](https://example.com)")
        MarkdownFormatOps.removeLink(in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "Hello")
    }

    func testLinkURLPrefillsFromTheSelection() {
        let text = content("[Hello](https://example.com)")
        XCTAssertEqual(MarkdownFormatOps.linkURL(text, at: whole(text)), "https://example.com")
        XCTAssertTrue(MarkdownFormatOps.hasLink(text, range: whole(text)))
    }

    func testLinkRangeCoversTheWholeLink() {
        let text = content("a [Hello](https://example.com) b")
        let range = MarkdownFormatOps.linkRange(text, at: 4)
        XCTAssertEqual(range, NSRange(location: 2, length: 5))
    }

    func testRetargetingALinkKeepsOneLink() {
        let text = content("[Hello](https://example.com)")
        MarkdownFormatOps.applyLink("https://other.example", in: text, range: whole(text))
        XCTAssertEqual(serialize(text), "[Hello](https://other.example)")
    }

    // MARK: - Idempotence

    func testFormattedOutputSurvivesASecondRoundTrip() {
        let text = content("Hello world")
        MarkdownFormatOps.toggleBold(in: text, range: NSRange(location: 0, length: 5))
        let once = serialize(text)
        let twice = MarkdownConversion.blocksToMarkdown(MarkdownConversion.markdownToBlocks(once))
        XCTAssertEqual(twice, once)
        XCTAssertEqual(once, "**Hello** world")
    }
}
