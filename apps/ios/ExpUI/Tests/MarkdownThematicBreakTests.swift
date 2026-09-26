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

    // MARK: - Deleting a break (one atom, exactly one newline)

    private func editorText(_ markdown: String) -> NSMutableAttributedString {
        let blocks = MarkdownConversion.markdownToBlocks(markdown)
        guard case .text(_, let content) = blocks[0] else { return NSMutableAttributedString() }
        return NSMutableAttributedString(attributedString: content)
    }

    private func delete(_ range: NSRange, from markdown: String) -> String {
        let text = editorText(markdown)
        let atom = MarkdownFormatOps.thematicBreakDeletionRange(in: text, deleting: range) ?? range
        text.replaceCharacters(in: atom, with: "")
        return MarkdownConversion.attributedStringToMarkdown(text)
    }

    func testBackspaceAtTheGlyphStartJoinsNothing() {
        // "A\n───\nB": the newline before the glyph is at 1.
        XCTAssertEqual(delete(NSRange(location: 1, length: 1), from: "A\n\n---\n\nB"), "A\n\nB")
    }

    func testBackspaceAtTheLineBelowRemovesTheBreak() {
        let after = 2 + (MarkdownStyle.thematicBreakGlyph as NSString).length
        XCTAssertEqual(delete(NSRange(location: after, length: 1), from: "A\n\n---\n\nB"), "A\n\nB")
    }

    func testDeletingAGlyphCharRemovesTheBreak() {
        XCTAssertEqual(delete(NSRange(location: 3, length: 1), from: "A\n\n---\n\nB"), "A\n\nB")
        XCTAssertEqual(delete(NSRange(location: 3, length: 1), from: "A\n\n---"), "A")
    }

    func testDeletionsAwayFromABreakAreUntouched() {
        let text = editorText("Ab\n\n---")
        XCTAssertNil(MarkdownFormatOps.thematicBreakDeletionRange(
            in: text, deleting: NSRange(location: 1, length: 1)))
    }

    // MARK: - Typing after the rule (the typing-attribute leak)

    private var glyphLength: Int { (MarkdownStyle.thematicBreakGlyph as NSString).length }

    /// The editor sanitizes typing attributes on every selection change; a
    /// caret parked after the glyph must not type the next paragraph AS the
    /// rule.
    func testSanitizedTypingAttributesDropTheBreakMarker() {
        let clean = MarkdownChipDecorator.sanitizedTypingAttributes(MarkdownStyle.thematicBreakAttributes)
        XCTAssertNil(clean[.markdownThematicBreak])
        XCTAssertEqual(clean[.foregroundColor] as? PlatformColor, MarkdownStyle.textColor)
        XCTAssertNotNil(clean[.font])
        let plain = MarkdownChipDecorator.sanitizedTypingAttributes(MarkdownStyle.baseAttributes)
        XCTAssertEqual(plain.count, MarkdownStyle.baseAttributes.count)
    }

    /// Typing `---`, then a paragraph, then Backspace at that paragraph's
    /// start removes ONLY the rule, whether the paragraph was typed with
    /// clean typing attributes or with the break's leaked ones.
    func testBackspaceAtTheStartOfTheParagraphTypedAfterTheRuleKeepsIt() {
        for leaked in [false, true] {
            let text = NSMutableAttributedString(string: "alpha\n--", attributes: MarkdownStyle.baseAttributes)
            guard let content = MarkdownFormatOps.thematicBreakShortcutRange(
                in: text, replacing: NSRange(location: 8, length: 0), with: "-") else {
                return XCTFail("expected the shortcut to fire")
            }
            let caret = MarkdownFormatOps.applyThematicBreak(to: text, content: content)
            let typing = leaked
                ? MarkdownStyle.thematicBreakAttributes
                : MarkdownChipDecorator.sanitizedTypingAttributes(MarkdownStyle.thematicBreakAttributes)
            if leaked {
                // The worst case: the newline after the glyph carries the marker too.
                text.setAttributes(typing, range: NSRange(location: caret - 1, length: 1))
            }
            text.insert(NSAttributedString(string: "beta", attributes: typing), at: caret)
            XCTAssertEqual(MarkdownConversion.attributedStringToMarkdown(text), "alpha\n\n---\n\nbeta", "leaked=\(leaked)")

            let backspace = NSRange(location: caret - 1, length: 1)
            let atom = MarkdownFormatOps.thematicBreakDeletionRange(in: text, deleting: backspace)
            XCTAssertEqual(atom, NSRange(location: 6, length: glyphLength + 1), "leaked=\(leaked)")
            text.replaceCharacters(in: atom ?? backspace, with: "")
            XCTAssertEqual(text.string, "alpha\nbeta", "leaked=\(leaked)")
            XCTAssertEqual(MarkdownConversion.attributedStringToMarkdown(text), "alpha\n\nbeta", "leaked=\(leaked)")
        }
    }

    /// Text that landed ON the glyph line with the marker (an older leak) is
    /// the paragraph below: the atom is the glyph alone, and a Backspace at
    /// the glyph's start keeps the newline that separates the two paragraphs.
    func testTextOnTheGlyphLineSurvivesDeletingTheRule() {
        func text() -> NSMutableAttributedString {
            let text = NSMutableAttributedString(string: "alpha\n", attributes: MarkdownStyle.baseAttributes)
            text.append(NSAttributedString(
                string: MarkdownStyle.thematicBreakGlyph + "beta", attributes: MarkdownStyle.thematicBreakAttributes))
            return text
        }
        XCTAssertEqual(MarkdownConversion.attributedStringToMarkdown(text()), "alpha\n\n---\n\nbeta")
        let glyph = NSRange(location: 6, length: glyphLength)

        let beforeBeta = text()
        let lastGlyphChar = NSRange(location: 6 + glyphLength - 1, length: 1)
        XCTAssertEqual(MarkdownFormatOps.thematicBreakDeletionRange(in: beforeBeta, deleting: lastGlyphChar), glyph)
        beforeBeta.replaceCharacters(in: glyph, with: "")
        XCTAssertEqual(MarkdownConversion.attributedStringToMarkdown(beforeBeta), "alpha\n\nbeta")

        let atGlyphStart = text()
        XCTAssertEqual(
            MarkdownFormatOps.thematicBreakDeletionRange(in: atGlyphStart, deleting: NSRange(location: 5, length: 1)),
            glyph)

        // Deleting the `b` itself leaves the rule alone.
        XCTAssertNil(MarkdownFormatOps.thematicBreakDeletionRange(
            in: text(), deleting: NSRange(location: 6 + glyphLength, length: 1)))
    }
}
