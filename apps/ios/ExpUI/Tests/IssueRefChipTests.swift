import Foundation
import UIKit
import XCTest
import ExpUI

/// EXP-322: a resolved `#IDENTIFIER` renders as `#EXP-42 <title>` WHILE
/// EDITING, like the web editor. The title rides a single `NSTextAttachment`
/// character, which `MarkdownConversion`'s serializer skips — so the contract
/// these tests defend is that the chip is *visible* and *serialization-
/// invisible* at the same time.
@MainActor
final class IssueRefChipTests: XCTestCase {

    private let resolver: (String) -> String? = { $0 == "EXP-42" ? "issue-id" : nil }
    private let titles: (String) -> String? = { $0 == "EXP-42" ? "Fix login flow" : nil }

    private func decorate(
        _ markdown: String,
        selection: NSRange = NSRange(location: 0, length: 0),
        titles: ((String) -> String?)? = nil
    ) -> MarkdownChipDecorator.Result {
        let blocks = MarkdownConversion.markdownToBlocks(markdown)
        guard case let .text(_, content) = blocks[0] else {
            fatalError("expected a leading text block")
        }
        return MarkdownChipDecorator.decorate(
            content,
            selection: selection,
            issueRefResolver: resolver,
            issueRefTitleResolver: titles ?? self.titles
        )
    }

    private func markdown(of attributed: NSAttributedString) -> String {
        MarkdownConversion.blocksToMarkdown([.text(id: UUID(), attributedContent: attributed)])
    }

    private func attachmentCount(in attributed: NSAttributedString) -> Int {
        var count = 0
        attributed.enumerateAttribute(
            .markdownIssueRefTitle,
            in: NSRange(location: 0, length: attributed.length),
            options: []
        ) { value, _, _ in if value != nil { count += 1 } }
        return count
    }

    // MARK: - The core contract

    func testTheChipTitleNeverReachesTheMarkdown() {
        let result = decorate("Fixes #EXP-42 today")
        XCTAssertTrue(result.changed)
        XCTAssertTrue(result.attributed.string.contains("\u{FFFC}"))
        XCTAssertEqual(markdown(of: result.attributed), "Fixes #EXP-42 today")
    }

    func testALoadedModelWithTitlesIsNotDirty() {
        let model = IssueEditorModel()
        model.issueRefResolver = resolver
        model.issueRefTitleResolver = titles
        let src = "Duplicate of #EXP-42, see also #EXP-7"
        model.load(markdown: src, baseURL: nil)
        XCTAssertEqual(model.currentMarkdown(), src)
        XCTAssertFalse(model.isDirty)
    }

    func testASecondPassReportsNoChange() {
        let first = decorate("Fixes #EXP-42 today")
        let second = MarkdownChipDecorator.decorate(
            first.attributed,
            selection: NSRange(location: 0, length: 0),
            issueRefResolver: resolver,
            issueRefTitleResolver: titles
        )
        XCTAssertFalse(second.changed)
        XCTAssertEqual(second.attributed.string, first.attributed.string)
        XCTAssertEqual(attachmentCount(in: second.attributed), 1)
    }

    func testTheAttachmentSitsAtTheTokenEndAndIsATapTarget() {
        let result = decorate("Fixes #EXP-42 today")
        let index = (result.attributed.string as NSString).range(of: "\u{FFFC}").location
        XCTAssertEqual(index, 13) // right after "Fixes #EXP-42"
        // Both halves of the chip carry the issue id, so tapping the title
        // navigates like tapping the identifier.
        XCTAssertEqual(
            result.attributed.attribute(.markdownIssueRef, at: index, effectiveRange: nil) as? String,
            "issue-id"
        )
        XCTAssertEqual(
            result.attributed.attribute(.markdownIssueRef, at: index - 1, effectiveRange: nil) as? String,
            "issue-id"
        )
    }

    func testUnresolvedIdentifiersStayPlainText() {
        let result = decorate("Fixes #EXP-99 today")
        XCTAssertFalse(result.changed)
        XCTAssertEqual(attachmentCount(in: result.attributed), 0)
        XCTAssertNil(result.attributed.attribute(.markdownChip, at: 7, effectiveRange: nil))
    }

    func testLongTitlesTruncateAtSixtyCharacters() {
        let long = String(repeating: "x", count: 80)
        let result = decorate("Fixes #EXP-42", titles: { _ in long })
        let index = (result.attributed.string as NSString).range(of: "\u{FFFC}").location
        let title = result.attributed.attribute(.markdownIssueRefTitle, at: index, effectiveRange: nil) as? String
        XCTAssertEqual(title, String(repeating: "x", count: 59) + "…")
    }

    func testABlankTitleKeepsTheBareToken() {
        let result = decorate("Fixes #EXP-42", titles: { _ in "   " })
        XCTAssertEqual(attachmentCount(in: result.attributed), 0)
    }

    // MARK: - Caret mapping

    func testACaretAtTheTokenEndStaysBeforeTheTitle() {
        // "Fixes #EXP-42 today", caret right after "42".
        let result = decorate("Fixes #EXP-42 today", selection: NSRange(location: 13, length: 0))
        XCTAssertEqual(result.selection.location, 13)
    }

    func testACaretBeforeTheTokenIsUnmoved() {
        let result = decorate("Fixes #EXP-42 today", selection: NSRange(location: 3, length: 0))
        XCTAssertEqual(result.selection.location, 3)
    }

    func testACaretAfterTheTokenShiftsByTheAttachment() {
        let result = decorate("Fixes #EXP-42 today", selection: NSRange(location: 16, length: 0))
        XCTAssertEqual(result.selection.location, 17)
    }

    // MARK: - Stale chips

    func testEditingAnIdentifierOutOfResolutionStripsItsChip() {
        let chipped = decorate("Fixes #EXP-42 today").attributed
        // The user types over the identifier so it no longer resolves.
        let edited = NSMutableAttributedString(attributedString: chipped)
        let tokenEnd = (chipped.string as NSString).range(of: "#EXP-42")
        edited.replaceCharacters(
            in: NSRange(location: NSMaxRange(tokenEnd) - 1, length: 1),
            with: NSAttributedString(string: "9", attributes: MarkdownStyle.baseAttributes)
        )
        let result = MarkdownChipDecorator.decorate(
            edited,
            selection: NSRange(location: 0, length: 0),
            issueRefResolver: resolver,
            issueRefTitleResolver: titles
        )
        XCTAssertEqual(attachmentCount(in: result.attributed), 0)
        XCTAssertNil(result.attributed.attribute(.markdownChip, at: 7, effectiveRange: nil))
        XCTAssertEqual(
            result.attributed.attribute(.foregroundColor, at: 7, effectiveRange: nil) as? PlatformColor,
            MarkdownStyle.textColor
        )
        XCTAssertEqual(markdown(of: result.attributed), "Fixes #EXP-49 today")
    }

    func testABlockquoteChipRestoresTheQuoteColorWhenUnchipped() {
        let blocks = MarkdownConversion.markdownToBlocks("> Fixes #EXP-42")
        guard case let .text(_, content) = blocks[0] else { return XCTFail("expected a text block") }
        let chipped = MarkdownChipDecorator.decorate(
            content,
            issueRefResolver: resolver,
            issueRefTitleResolver: titles
        ).attributed
        let plain = MarkdownChipDecorator.decorate(chipped, issueRefResolver: { _ in nil }).attributed
        let tokenStart = (plain.string as NSString).range(of: "#EXP-42").location
        XCTAssertEqual(
            plain.attribute(.foregroundColor, at: tokenStart, effectiveRange: nil) as? PlatformColor,
            MarkdownStyle.blockquoteTextColor
        )
    }

    // MARK: - Skipped contexts

    func testTokensInsideCodeAndLinksAreNeverChipped() {
        for source in ["Fixes `#EXP-42` today", "```\n#EXP-42\n```", "See [#EXP-42](https://x.test)"] {
            let blocks = MarkdownConversion.markdownToBlocks(source)
            guard case let .text(_, content) = blocks[0] else { continue }
            let result = MarkdownChipDecorator.decorate(
                content,
                issueRefResolver: resolver,
                issueRefTitleResolver: titles
            )
            XCTAssertEqual(attachmentCount(in: result.attributed), 0, "chipped inside: \(source)")
        }
    }

    // MARK: - Round trip

    func testDecoratedBlocksRoundTripByteIdenticallyTwice() {
        let src = "Fixes #EXP-42 today\n\n- item with #EXP-42\n- plain item"
        let model = IssueEditorModel()
        model.issueRefResolver = resolver
        model.issueRefTitleResolver = titles
        model.load(markdown: src, baseURL: nil)
        let once = model.currentMarkdown()
        XCTAssertEqual(once, src)
        model.load(markdown: once, baseURL: nil)
        XCTAssertEqual(model.currentMarkdown(), once)
    }

    // MARK: - Typing attributes

    func testSanitizedTypingAttributesStripTheChipAndRestoreTheColor() {
        var attrs = MarkdownStyle.baseAttributes
        attrs[.markdownChip] = true
        attrs[.markdownIssueRef] = "issue-id"
        attrs[.markdownChipBaseColor] = MarkdownStyle.blockquoteTextColor
        attrs[.foregroundColor] = MarkdownStyle.linkColor
        let clean = MarkdownChipDecorator.sanitizedTypingAttributes(attrs)
        XCTAssertNil(clean[.markdownChip])
        XCTAssertNil(clean[.markdownIssueRef])
        XCTAssertNil(clean[.markdownChipBaseColor])
        XCTAssertEqual(clean[.foregroundColor] as? PlatformColor, MarkdownStyle.blockquoteTextColor)
        // Untouched attributes survive, so lists / headings keep working.
        XCTAssertNotNil(clean[.paragraphStyle])
    }

    func testSanitizingLeavesUnchippedAttributesAlone() {
        let attrs = MarkdownStyle.baseAttributes
        let clean = MarkdownChipDecorator.sanitizedTypingAttributes(attrs)
        XCTAssertEqual(clean[.foregroundColor] as? PlatformColor, MarkdownStyle.textColor)
    }

    // MARK: - Chip-atom deletion

    func testTheChipDeletesAsOneAtom() {
        let chipped = decorate("Fixes #EXP-42 today").attributed
        let attachment = (chipped.string as NSString).range(of: "\u{FFFC}")
        let atom = MarkdownChipDecorator.chipAtomRange(in: chipped, endingAt: NSMaxRange(attachment))
        XCTAssertEqual(atom, NSRange(location: 6, length: 8)) // "#EXP-42" + the attachment
    }

    func testThereIsNoChipAtomAwayFromAChip() {
        let chipped = decorate("Fixes #EXP-42 today").attributed
        XCTAssertNil(MarkdownChipDecorator.chipAtomRange(in: chipped, endingAt: 4))
        XCTAssertNil(MarkdownChipDecorator.chipAtomRange(in: chipped, endingAt: 0))
    }
}
