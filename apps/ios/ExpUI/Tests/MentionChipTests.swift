import Foundation
import UIKit
import XCTest
import ExpUI

/// EXP-322: resolved `@email` mentions get the chip styling in the editor but
/// keep their exact characters — web's `mention-pill-extension.ts` does the
/// same, because "hiding characters under an active caret makes editing
/// hazardous". Only the read-only comment renderer may substitute names.
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
            MarkdownStyle.linkColor
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
