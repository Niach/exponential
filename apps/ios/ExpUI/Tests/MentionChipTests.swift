import Foundation
import UIKit
import XCTest
import ExpUI

/// EXP-322: resolved `@email` mentions get the chip styling in the editor but
/// keep their exact characters — web's `mention-pill-extension.ts` does the
/// same, because "hiding characters under an active caret makes editing
/// hazardous". Only the read-only comment renderer substitutes names
/// (`MentionDisplayTests`, EXP-713).
final class MentionRefsTests: XCTestCase {

    private func emails(in text: String) -> [String] {
        MentionRefs.matches(in: text).map(\.email)
    }

    func testMatchesTheWebMentionSource() {
        XCTAssertEqual(emails(in: "Ping @ada@example.com about it"), ["ada@example.com"])
        XCTAssertEqual(emails(in: "@a.b-c@sub.example.co.uk owns this"), ["a.b-c@sub.example.co.uk"])
    }

    func testABareAddressIsNotAMention() {
        XCTAssertEqual(emails(in: "mail ada@example.com"), [])
    }

    func testSkipsInlineCodeAndFencedBlocks() {
        XCTAssertEqual(emails(in: "see `@ada@example.com`"), [])
        XCTAssertEqual(emails(in: "```\n@ada@example.com\n```"), [])
    }
}

@MainActor
final class MentionChipDecorationTests: XCTestCase {

    private let names: (String) -> String? = {
        $0 == "ada@example.com" ? "Ada Lovelace" : nil
    }

    private func decorate(_ markdown: String) -> NSAttributedString {
        let blocks = MarkdownConversion.markdownToBlocks(markdown)
        guard case let .text(_, content) = blocks[0] else { fatalError("expected a text block") }
        return MentionRefs.decorate(content, resolver: names)
    }

    /// The contract: chipping a mention must never change a character.
    func testChippingAMentionNeverChangesTheCharacters() {
        let src = "Ping @ada@example.com about it"
        let decorated = decorate(src)
        XCTAssertEqual(decorated.string, src)
        XCTAssertEqual(
            MarkdownConversion.blocksToMarkdown([.text(id: UUID(), attributedContent: decorated)]),
            src
        )
    }

    func testAResolvedMentionIsChipped() {
        let decorated = decorate("Ping @ada@example.com about it")
        let start = (decorated.string as NSString).range(of: "@ada@example.com").location
        XCTAssertEqual(
            decorated.attribute(.markdownMention, at: start, effectiveRange: nil) as? String,
            "Ada Lovelace"
        )
        XCTAssertEqual(decorated.attribute(.markdownChip, at: start, effectiveRange: nil) as? Bool, true)
        XCTAssertEqual(
            decorated.attribute(.foregroundColor, at: start, effectiveRange: nil) as? PlatformColor,
            MarkdownStyle.mentionForeground
        )
    }

    func testAnUnknownAddressStaysPlain() {
        let decorated = decorate("Ping @nobody@example.com about it")
        let start = (decorated.string as NSString).range(of: "@nobody@example.com").location
        XCTAssertNil(decorated.attribute(.markdownChip, at: start, effectiveRange: nil))
    }

    func testTheEditorPassChipsMentionsAndIssueRefsTogether() {
        let model = IssueEditorModel()
        model.issueRefResolver = { $0 == "EXP-42" ? "issue-id" : nil }
        model.issueRefTitleResolver = { $0 == "EXP-42" ? "Fix login flow" : nil }
        model.mentionMembers = [MentionMember(name: "Ada Lovelace", email: "ada@example.com")]
        let src = "Ping @ada@example.com about #EXP-42"
        model.load(markdown: src, baseURL: nil)
        XCTAssertEqual(model.currentMarkdown(), src)
        guard case let .text(_, content) = model.blocks[0] else { return XCTFail("expected a text block") }
        let mentionStart = (content.string as NSString).range(of: "@ada@example.com").location
        XCTAssertNotNil(content.attribute(.markdownMention, at: mentionStart, effectiveRange: nil))
        XCTAssertTrue(content.string.contains("\u{FFFC}"))
    }
}

/// Session e4a04d22: the pill is ONE look ×4 — web `.mention-pill`: the
/// `--accent` fill, the `--border` hairline, `--accent-foreground` text at
/// 13 medium, `0.4rem` side padding. The colours come off the shared design
/// tokens; this locks both the values and that both paths carry them.
@MainActor
final class MentionPillLookTests: XCTestCase {

    private func rgba(_ color: PlatformColor) -> [Int] {
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        color.getRed(&r, green: &g, blue: &b, alpha: &a)
        return [r, g, b, a].map { Int(($0 * 255).rounded()) }
    }

    private func content(_ markdown: String) -> NSAttributedString {
        let blocks = MarkdownConversion.markdownToBlocks(markdown)
        guard case let .text(_, content) = blocks[0] else { fatalError("expected a text block") }
        return content
    }

    func testThePaletteIsTheWebTokens() {
        // oklch(0.269 0 0) / oklch(0.985 0 0) / oklch(1 0 0 / 10%).
        XCTAssertEqual(rgba(MarkdownStyle.mentionBackground), [38, 38, 38, 255])
        XCTAssertEqual(rgba(MarkdownStyle.mentionForeground), [250, 250, 250, 255])
        XCTAssertEqual(rgba(MarkdownStyle.mentionBorder), [255, 255, 255, 26])
        XCTAssertEqual(MarkdownStyle.mentionFontSize, 13)
        XCTAssertEqual(MarkdownStyle.mentionPaddingX, 6.4, accuracy: 0.001)
        XCTAssertEqual(MarkdownStyle.mentionBorderWidth, 1)
    }

    func testTheEditorPillRestoresTheBodyWhenItStopsResolving() {
        let src = "Ping @ada@example.com about it"
        let base = content(src)
        let chipped = MarkdownChipDecorator.decorate(
            base, issueRefResolver: nil, mentionResolver: { _ in "Ada Lovelace" }).attributed
        let start = (chipped.string as NSString).range(of: "@ada").location
        XCTAssertEqual(
            (chipped.attribute(.font, at: start, effectiveRange: nil) as? PlatformFont)?.pointSize ?? 0,
            MarkdownStyle.mentionFont.pointSize, accuracy: 0.01)
        XCTAssertEqual(chipped.attribute(.kern, at: start - 1, effectiveRange: nil) as? CGFloat,
                       MarkdownStyle.mentionPaddingX)
        // Idempotent: a second pass changes nothing.
        XCTAssertFalse(MarkdownChipDecorator.decorate(
            chipped, issueRefResolver: nil, mentionResolver: { _ in "Ada Lovelace" }).changed)
        // The member leaves: font, colour and both pads go back.
        let plain = MarkdownChipDecorator.decorate(
            chipped, issueRefResolver: nil, mentionResolver: { _ in nil }).attributed
        XCTAssertTrue(plain.isEqual(to: base))
        XCTAssertEqual(
            MarkdownConversion.blocksToMarkdown([.text(id: UUID(), attributedContent: chipped)]), src)
    }
}
