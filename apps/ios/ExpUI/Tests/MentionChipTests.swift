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

/// EXP-713: the read-only comment card renders a resolved mention as the
/// member's NAME pill — `@Jonas Weber`, not `@jonas@acme.dev` — like web's
/// read-only widget and Android's `MentionDisplay`. The stored markdown is
/// never touched because display-only models never serialize.
@MainActor
final class MentionDisplayTests: XCTestCase {

    private let names: (String) -> String? = {
        $0 == "ada@example.com" ? "Ada Lovelace" : nil
    }

    private func content(_ markdown: String) -> NSAttributedString {
        let blocks = MarkdownConversion.markdownToBlocks(markdown)
        guard case let .text(_, content) = blocks[0] else { fatalError("expected a text block") }
        return content
    }

    func testAResolvedMentionShowsTheNameNotTheAddress() {
        let decorated = MentionRefs.decorateForDisplay(content("Ping @ada@example.com about it"), resolver: names)
        XCTAssertEqual(decorated.string, "Ping @Ada\u{00A0}Lovelace about it")
        let start = (decorated.string as NSString).range(of: "@Ada").location
        XCTAssertEqual(
            decorated.attribute(.markdownMention, at: start, effectiveRange: nil) as? String,
            "Ada Lovelace"
        )
        XCTAssertEqual(decorated.attribute(.markdownChip, at: start, effectiveRange: nil) as? Bool, true)
        XCTAssertEqual(
            decorated.attribute(.foregroundColor, at: start, effectiveRange: nil) as? PlatformColor,
            MarkdownStyle.linkColor
        )
        // The pill ends exactly with the name: the text after it is plain.
        let after = start + ("@Ada\u{00A0}Lovelace" as NSString).length
        XCTAssertNil(decorated.attribute(.markdownChip, at: after, effectiveRange: nil))
    }

    func testTheNameIsOneUnbreakableUnit() {
        // Web parity: `.mention-pill { white-space: nowrap }` — a wrapped name
        // would otherwise split into two pills.
        XCTAssertEqual(MentionRefs.displayText(name: "Ada  King Lovelace", email: "ada@example.com"),
                       "@Ada\u{00A0}King\u{00A0}Lovelace")
        XCTAssertEqual(MentionRefs.displayText(name: "  ", email: "ada@example.com"), "@ada@example.com")
    }

    func testAnUnknownAddressKeepsTheStoredToken() {
        let src = "Ping @nobody@example.com about it"
        let decorated = MentionRefs.decorateForDisplay(content(src), resolver: names)
        XCTAssertEqual(decorated.string, src)
        let start = (decorated.string as NSString).range(of: "@nobody").location
        XCTAssertNil(decorated.attribute(.markdownChip, at: start, effectiveRange: nil))
    }

    func testCodeAndLinksAreNeverSubstituted() {
        let src = "see `@ada@example.com` and [@ada@example.com](https://example.com)"
        let decorated = MentionRefs.decorateForDisplay(content(src), resolver: names)
        XCTAssertEqual(decorated.string, content(src).string)
    }

    func testASecondPassIsIdempotent() {
        // The member list syncs in after the load and re-runs the pass; the
        // substituted pill must not be touched again (a name that itself looks
        // like an address is the only way the regex could re-match it).
        let resolver: (String) -> String? = { $0 == "ada@example.com" ? "bot@example.com" : nil }
        let once = MentionRefs.decorateForDisplay(content("Ping @ada@example.com"), resolver: resolver)
        XCTAssertEqual(once.string, "Ping @bot@example.com")
        let twice = MentionRefs.decorateForDisplay(once, resolver: { _ in "Nope" })
        XCTAssertEqual(twice.string, once.string)
    }

    func testTheDisplayModelSubstitutesButTheEditableModelDoesNot() {
        let src = "Ping @ada@example.com about #EXP-42"
        let members = [MentionMember(name: "Ada Lovelace", email: "ada@example.com")]

        let display = IssueEditorModel()
        display.isDisplayOnly = true
        display.issueRefResolver = { $0 == "EXP-42" ? "issue-id" : nil }
        display.issueRefTitleResolver = { $0 == "EXP-42" ? "Fix login flow" : nil }
        display.mentionMembers = members
        display.load(markdown: src, baseURL: nil)
        guard case let .text(_, shown) = display.blocks[0] else { return XCTFail("expected a text block") }
        XCTAssertEqual(shown.string, "Ping @Ada\u{00A0}Lovelace about #EXP-42 Fix login flow")

        let editable = IssueEditorModel()
        editable.issueRefResolver = display.issueRefResolver
        editable.issueRefTitleResolver = display.issueRefTitleResolver
        editable.mentionMembers = members
        editable.load(markdown: src, baseURL: nil)
        XCTAssertEqual(editable.currentMarkdown(), src)
        guard case let .text(_, edited) = editable.blocks[0] else { return XCTFail("expected a text block") }
        XCTAssertTrue(edited.string.contains("@ada@example.com"))
    }

    func testLateSyncingMembersChipAnAlreadyLoadedCard() {
        let display = IssueEditorModel()
        display.isDisplayOnly = true
        display.issueRefResolver = { _ in nil }
        display.issueRefTitleResolver = { _ in nil }
        display.load(markdown: "Ping @ada@example.com", baseURL: nil)
        guard case let .text(_, before) = display.blocks[0] else { return XCTFail("expected a text block") }
        XCTAssertEqual(before.string, "Ping @ada@example.com")
        display.mentionMembers = [MentionMember(name: "Ada Lovelace", email: "ada@example.com")]
        guard case let .text(_, after) = display.blocks[0] else { return XCTFail("expected a text block") }
        XCTAssertEqual(after.string, "Ping @Ada\u{00A0}Lovelace")
    }
}
