import Foundation
import XCTest
import ExpUI

// EXP-1018: a `---` thematic break round-trips through the editor, and typing
// the third `-` of a lone `---` turns the line into one at once (web's input
// rule, desktop's separator block).
final class MarkdownThematicBreakTests: XCTestCase {
    private func roundTrip(_ markdown: String) -> String {
        MarkdownConversion.blocksToMarkdown(MarkdownConversion.markdownToBlocks(markdown))
    }

    func testThematicBreakRoundTrips() {
        XCTAssertEqual(roundTrip("First\n\n---\n\nSecond"), "First\n\n---\n\nSecond")
        XCTAssertEqual(roundTrip("---\n\nSecond"), "---\n\nSecond")
        XCTAssertEqual(roundTrip("First\n\n---"), "First\n\n---")
    }

    func testTextTypedAfterTheGlyphBecomesTheNextParagraph() {
        let text = NSMutableAttributedString(
            string: MarkdownStyle.thematicBreakGlyph + "tail",
            attributes: MarkdownStyle.thematicBreakAttributes)
        XCTAssertEqual(MarkdownConversion.attributedStringToMarkdown(text), "---\n\ntail")
    }

    func testThirdDashTriggersTheShortcut() {
        let text = NSMutableAttributedString(string: "First\n--", attributes: MarkdownStyle.baseAttributes)
        let caret = NSRange(location: text.length, length: 0)
        guard let content = MarkdownFormatOps.thematicBreakShortcutRange(in: text, replacing: caret, with: "-") else {
            return XCTFail("expected the shortcut to fire")
        }
        XCTAssertEqual(content, NSRange(location: 6, length: 2))
        let location = MarkdownFormatOps.applyThematicBreak(to: text, content: content)
        XCTAssertEqual(location, text.length)
        XCTAssertEqual(text.string, "First\n" + MarkdownStyle.thematicBreakGlyph + "\n")
        XCTAssertEqual(MarkdownConversion.attributedStringToMarkdown(text), "First\n\n---")
    }

    func testSmartDashAndDashTriggersTheShortcut() {
        let text = NSMutableAttributedString(string: "\u{2014}", attributes: MarkdownStyle.baseAttributes)
        XCTAssertNotNil(MarkdownFormatOps.thematicBreakShortcutRange(
            in: text, replacing: NSRange(location: 1, length: 0), with: "-"))
    }

    func testShortcutKeepsTheFollowingLine() {
        let text = NSMutableAttributedString(string: "--\nNext", attributes: MarkdownStyle.baseAttributes)
        let content = MarkdownFormatOps.thematicBreakShortcutRange(
            in: text, replacing: NSRange(location: 2, length: 0), with: "-")
        XCTAssertEqual(content, NSRange(location: 0, length: 2))
        let location = MarkdownFormatOps.applyThematicBreak(to: text, content: content!)
        XCTAssertEqual(location, (MarkdownStyle.thematicBreakGlyph as NSString).length + 1)
        XCTAssertEqual(MarkdownConversion.attributedStringToMarkdown(text), "---\n\nNext")
    }

    func testShortcutIgnoresOtherLines() {
        func fires(_ string: String, at location: Int? = nil, attrs: [NSAttributedString.Key: Any] = MarkdownStyle.baseAttributes) -> Bool {
            let text = NSAttributedString(string: string, attributes: attrs)
            let caret = NSRange(location: location ?? text.length, length: 0)
            return MarkdownFormatOps.thematicBreakShortcutRange(in: text, replacing: caret, with: "-") != nil
        }
        XCTAssertFalse(fires("-"))
        XCTAssertFalse(fires("a--"))
        XCTAssertFalse(fires("---"))
        XCTAssertFalse(fires("--", at: 1))
        var code = MarkdownStyle.baseAttributes
        code[.markdownCodeBlock] = true
        XCTAssertFalse(fires("--", attrs: code))
        var heading = MarkdownStyle.baseAttributes
        heading[.markdownHeadingLevel] = 2
        XCTAssertFalse(fires("--", attrs: heading))
    }
}
