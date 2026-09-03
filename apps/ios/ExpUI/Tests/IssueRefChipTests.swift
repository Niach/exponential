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
        titles: ((String) -> String?)? = nil,
        statuses: ((String) -> IssueRefStatusInfo?)? = nil
    ) -> MarkdownChipDecorator.Result {
        let blocks = MarkdownConversion.markdownToBlocks(markdown)
        guard case let .text(_, content) = blocks[0] else {
            fatalError("expected a leading text block")
        }
        return MarkdownChipDecorator.decorate(
            content,
            selection: selection,
            issueRefResolver: resolver,
            issueRefTitleResolver: titles ?? self.titles,
            issueRefStatusResolver: statuses
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
        attrs[.kern] = MarkdownStyle.chipStatusIconGap
        let clean = MarkdownChipDecorator.sanitizedTypingAttributes(attrs)
        XCTAssertNil(clean[.markdownChip])
        XCTAssertNil(clean[.markdownIssueRef])
        XCTAssertNil(clean[.markdownChipBaseColor])
        // The status glyph's cell gap must not ride into typed text (EXP-655).
        XCTAssertNil(clean[.kern])
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

    /// A hardware forward-delete with the caret parked between the token and
    /// its title reaches the delegate as the SAME replacement range as a
    /// backspace from the chip's right edge — so the one atom rule covers both
    /// directions and neither can "stick" on the re-inserted attachment.
    func testForwardDeletingIntoAChipHitsTheSameAtom() throws {
        let chipped = decorate("Fixes #EXP-42 today").attributed
        let attachment = (chipped.string as NSString).range(of: "\u{FFFC}")
        let forwardDelete = NSRange(location: attachment.location, length: 1)
        let atom = try XCTUnwrap(
            MarkdownChipDecorator.chipAtomRange(in: chipped, endingAt: NSMaxRange(forwardDelete)))
        XCTAssertEqual(atom, NSRange(location: 6, length: 8))
        // The editor's guard: the atom starts before the deletion, so it takes
        // over instead of letting UIKit remove the attachment alone.
        XCTAssertLessThan(atom.location, forwardDelete.location)
    }

    // MARK: - Tables (EXP-322)

    /// The verbatim pipe-table emitter re-emits its SOURCE STRING line for line
    /// without consulting attributes, so a chip attachment spliced into a table
    /// cell is not skipped — it is SAVED. Nothing inside a table may be chipped.
    func testATableCellRefNeverLeaksAChipIntoTheMarkdown() {
        let src = "| Ticket | Owner |\n| --- | --- |\n| #EXP-42 | ada |"
        let undecorated = MarkdownConversion.blocksToMarkdown(
            MarkdownConversion.markdownToBlocks(src))
        let model = IssueEditorModel()
        model.issueRefResolver = resolver
        model.issueRefTitleResolver = titles
        model.load(markdown: src, baseURL: nil)
        XCTAssertFalse(model.currentMarkdown().contains("\u{FFFC}"))
        XCTAssertEqual(model.currentMarkdown(), undecorated)
        // ...and the silent-corruption half: a decorated load must not read as
        // clean while holding text that differs from what was loaded.
        XCTAssertFalse(model.isDirty)
    }

    func testTokensInsideATableAreNeverChipped() {
        let blocks = MarkdownConversion.markdownToBlocks("| Ticket |\n| --- |\n| #EXP-42 |")
        guard case let .text(_, content) = blocks[0] else { return XCTFail("expected a text block") }
        let result = MarkdownChipDecorator.decorate(
            content,
            issueRefResolver: resolver,
            issueRefTitleResolver: titles
        )
        XCTAssertFalse(result.changed)
        XCTAssertEqual(attachmentCount(in: result.attributed), 0)
    }

    // MARK: - Bare object-replacement characters (EXP-322)

    /// Copy/paste of a chip re-enters the document as the PLAIN text
    /// `#EXP-42\u{FFFC}` — no attachment attribute, so the serializer's
    /// attachment skip does not apply. The emitters strip U+FFFC unconditionally
    /// (it has no legitimate place in GFM source), which is the one chokepoint
    /// no paste or decoration path can bypass.
    func testABarePastedObjectReplacementNeverReachesTheMarkdown() {
        let pasted = NSAttributedString(
            string: "Fixes #EXP-42\u{FFFC} today", attributes: MarkdownStyle.baseAttributes)
        XCTAssertEqual(markdown(of: pasted), "Fixes #EXP-42 today")

        // The next decoration pass adds a REAL attachment beside the bare one:
        // two U+FFFC, only one of them attributed. Still zero bytes.
        let result = MarkdownChipDecorator.decorate(
            pasted,
            issueRefResolver: resolver,
            issueRefTitleResolver: titles
        )
        XCTAssertEqual(result.attributed.string.filter { $0 == "\u{FFFC}" }.count, 2)
        XCTAssertEqual(markdown(of: result.attributed), "Fixes #EXP-42 today")
    }

    func testABareObjectReplacementInsideACodeFenceIsStripped() {
        var attrs = MarkdownStyle.baseAttributes
        attrs[.markdownCodeBlock] = true
        let content = NSAttributedString(string: "let x = 1\u{FFFC}", attributes: attrs)
        XCTAssertEqual(markdown(of: content), "```\nlet x = 1\n```")
    }

    /// EXP-726: table cells are real editable runs now, so a `#EXP-42` inside
    /// one gets the same title attachment as anywhere else — and, exactly as
    /// anywhere else, it must not reach the markdown.
    func testADecoratedIssueRefInsideATableCellNeverReachesTheMarkdown() {
        let blocks = MarkdownConversion.markdownToBlocks(
            "| Ticket |\n| --- |\n| #EXP-42 |")
        guard let table = blocks.compactMap({ block -> TableBlock? in
            if case let .table(_, table) = block { return table }
            return nil
        }).first else { return XCTFail("expected a table block") }

        var decorated = table
        _ = decorated.transformCells { cell in
            let result = MarkdownChipDecorator.decorate(
                cell.content,
                issueRefResolver: resolver,
                issueRefTitleResolver: titles
            )
            return result.changed ? result.attributed : nil
        }
        let cell = decorated.rows[0][0].content
        XCTAssertTrue(
            cell.string.contains("\u{FFFC}"),
            "the chip title must be visible inside the cell")
        XCTAssertEqual(
            MarkdownConversion.blocksToMarkdown([.table(id: UUID(), table: decorated)]),
            "| Ticket |\n| --- |\n| #EXP-42 |")
    }

    /// A BARE U+FFFC pasted into a cell (the attachment does not survive the
    /// pasteboard) is stripped by the serializer's one chokepoint too.
    func testABareObjectReplacementInsideATableCellIsStripped() {
        let table = TableBlock(
            header: [TableCell(content: NSAttributedString(
                string: "Ticket", attributes: MarkdownStyle.baseAttributes))],
            rows: [[TableCell(content: NSAttributedString(
                string: "#EXP-42\u{FFFC}", attributes: MarkdownStyle.baseAttributes))]]
        )
        XCTAssertEqual(
            MarkdownConversion.blocksToMarkdown([.table(id: UUID(), table: table)]),
            "| Ticket |\n| --- |\n| #EXP-42 |")
    }

    // MARK: - Status glyph (EXP-423)

    private let statuses: (String) -> IssueRefStatusInfo? = {
        $0 == "EXP-42" ? IssueRefStatusInfo(iconName: "circle-dashed", color: .systemBlue) : nil
    }

    /// The chip's status glyph is painted OVER the `#`, which is hidden by
    /// clearing that one character's color — the zero-bytes rule holds by
    /// construction because no character moves.
    func testTheStatusGlyphHidesTheHashWithoutMovingACharacter() {
        let result = decorate("Fixes #EXP-42 today", statuses: statuses)
        let hash = (result.attributed.string as NSString).range(of: "#EXP-42").location
        XCTAssertEqual(
            result.attributed.attribute(.markdownIssueRefStatus, at: hash, effectiveRange: nil)
                as? IssueRefStatusInfo,
            statuses("EXP-42")
        )
        XCTAssertEqual(
            result.attributed.attribute(.foregroundColor, at: hash, effectiveRange: nil) as? PlatformColor,
            PlatformColor.clear
        )
        // Only the `#` is hidden — the identifier keeps the muted token color
        // (Linear look: web/Android/desktop parity).
        XCTAssertEqual(
            result.attributed.attribute(.foregroundColor, at: hash + 1, effectiveRange: nil) as? PlatformColor,
            MarkdownStyle.chipTokenColor
        )
        // EXP-655: that one cell is kerned wider so the glyph clears the
        // identifier — advance only, still no character moves.
        XCTAssertEqual(
            result.attributed.attribute(.kern, at: hash, effectiveRange: nil) as? CGFloat,
            MarkdownStyle.chipStatusIconGap
        )
        XCTAssertNil(result.attributed.attribute(.kern, at: hash + 1, effectiveRange: nil))
        XCTAssertEqual(markdown(of: result.attributed), "Fixes #EXP-42 today")
    }

    /// The decorator's `changed` flag is an `NSAttributedString.isEqual`, so a
    /// status info that doesn't compare equal would make the editor rewrite its
    /// storage on every keystroke forever.
    func testASecondPassWithAStatusResolverReportsNoChange() {
        let first = decorate("Fixes #EXP-42 today", statuses: statuses)
        XCTAssertTrue(first.changed)
        let second = MarkdownChipDecorator.decorate(
            first.attributed,
            issueRefResolver: resolver,
            issueRefTitleResolver: titles,
            issueRefStatusResolver: statuses
        )
        XCTAssertFalse(second.changed)
        XCTAssertEqual(second.attributed.string, first.attributed.string)
        XCTAssertEqual(attachmentCount(in: second.attributed), 1)
        XCTAssertEqual(markdown(of: second.attributed), "Fixes #EXP-42 today")
    }

    func testAStatusInfoComparesOnBothFields() {
        let info = IssueRefStatusInfo(iconName: "circle", color: .systemBlue)
        XCTAssertEqual(info, IssueRefStatusInfo(iconName: "circle", color: .systemBlue))
        XCTAssertNotEqual(info, IssueRefStatusInfo(iconName: "circle-check", color: .systemBlue))
        XCTAssertNotEqual(info, IssueRefStatusInfo(iconName: "circle", color: .systemRed))
    }

    /// The hidden `#` splits the token's color run — the atom must still start
    /// at the `#`, or a backspace at the chip's right edge would leave one.
    func testAStatusChipStillDeletesAsOneAtom() {
        let chipped = decorate("Fixes #EXP-42 today", statuses: statuses).attributed
        let attachment = (chipped.string as NSString).range(of: "\u{FFFC}")
        XCTAssertEqual(
            MarkdownChipDecorator.chipAtomRange(in: chipped, endingAt: NSMaxRange(attachment)),
            NSRange(location: 6, length: 8)
        )
    }

    func testUnchippingRestoresTheHiddenHash() {
        let chipped = decorate("Fixes #EXP-42 today", statuses: statuses).attributed
        let plain = MarkdownChipDecorator.decorate(chipped, issueRefResolver: { _ in nil }).attributed
        let hash = (plain.string as NSString).range(of: "#EXP-42").location
        XCTAssertNil(plain.attribute(.markdownIssueRefStatus, at: hash, effectiveRange: nil))
        XCTAssertEqual(
            plain.attribute(.foregroundColor, at: hash, effectiveRange: nil) as? PlatformColor,
            MarkdownStyle.textColor
        )
        // And the glyph's kerned cell (EXP-655) — a leftover would split the
        // token's attribute run and block the next decoration pass.
        XCTAssertNil(plain.attribute(.kern, at: hash, effectiveRange: nil))
    }

    func testTheDisplayPathHidesTheHashToo() {
        let blocks = MarkdownConversion.markdownToBlocks("Fixes #EXP-42 today")
        guard case let .text(_, content) = blocks[0] else { return XCTFail("expected a text block") }
        let decorated = IssueRefs.decorateForDisplay(
            content, resolver: resolver, titleResolver: titles, statusResolver: statuses)
        XCTAssertTrue(decorated.string.contains("#EXP-42 Fix login flow"))
        let hash = (decorated.string as NSString).range(of: "#EXP-42").location
        XCTAssertEqual(
            decorated.attribute(.foregroundColor, at: hash, effectiveRange: nil) as? PlatformColor,
            PlatformColor.clear
        )
        XCTAssertEqual(
            decorated.attribute(.kern, at: hash, effectiveRange: nil) as? CGFloat,
            MarkdownStyle.chipStatusIconGap
        )
        XCTAssertNil(decorated.attribute(.kern, at: hash + 1, effectiveRange: nil))
        XCTAssertNotNil(decorated.attribute(.markdownIssueRefStatus, at: hash, effectiveRange: nil))
    }

    func testStatusChipsStillRoundTripByteIdentically() {
        let src = "Fixes #EXP-42 today\n\n- item with #EXP-42\n- plain item"
        let model = IssueEditorModel()
        model.issueRefResolver = resolver
        model.issueRefTitleResolver = titles
        model.issueRefStatusResolver = statuses
        model.load(markdown: src, baseURL: nil)
        XCTAssertEqual(model.currentMarkdown(), src)
        XCTAssertFalse(model.isDirty)
    }

    // MARK: - Selection mapping

    /// The pass maps the whole selection, not just the caret — collapsing it
    /// would drop the user's selection whenever a ref happened to resolve.
    func testASelectionSpanningTheTokenKeepsItsLength() {
        let result = decorate("Fixes #EXP-42 today", selection: NSRange(location: 0, length: 19))
        XCTAssertEqual(result.selection, NSRange(location: 0, length: 20))
    }

    // MARK: - Caret snapping (EXP-655)

    /// `"Fixes #EXP-42 today"` decorates to token 6..<13, title attachment at
    /// 13, atom end 14 — the same geometry `testTheChipDeletesAsOneAtom` pins.
    private func snap(
        _ chipped: NSAttributedString, _ proposed: Int, previous: Int? = nil, allowSeam: Bool = false
    ) -> Int {
        MarkdownChipDecorator.snappedCaret(
            in: chipped, proposed: proposed, previous: previous, allowSeam: allowSeam)
    }

    func testACaretMovingRightLeavesTheChipOnItsFarSide() {
        let chipped = decorate("Fixes #EXP-42 today").attributed
        XCTAssertEqual(snap(chipped, 7, previous: 6), 14)
    }

    func testACaretMovingLeftLeavesTheChipOnItsNearSide() {
        let chipped = decorate("Fixes #EXP-42 today").attributed
        XCTAssertEqual(snap(chipped, 12, previous: 13), 6)
        XCTAssertEqual(snap(chipped, 13, previous: 14), 6)
    }

    /// A tap has no direction of travel — wherever the caret was before, it
    /// falls out the nearer edge; only a one-character step is a move.
    func testACaretWithoutADirectionFallsOutTheNearerEdge() {
        let chipped = decorate("Fixes #EXP-42 today").attributed
        XCTAssertEqual(snap(chipped, 7), 6)
        XCTAssertEqual(snap(chipped, 12), 14)
        XCTAssertEqual(snap(chipped, 12, previous: 0), 14)
        XCTAssertEqual(snap(chipped, 7, previous: 19), 6)
    }

    /// The seam between identifier and title is visually inside the pill, so it
    /// is legal only for the programmatic caret the decorator parks there (which
    /// is what keeps typing extending the token).
    func testTheSeamIsLegalOnlyForProgrammaticMoves() {
        let chipped = decorate("Fixes #EXP-42 today").attributed
        XCTAssertEqual(snap(chipped, 13, allowSeam: true), 13)
        XCTAssertEqual(snap(chipped, 13, previous: 14), 6)
        XCTAssertEqual(snap(chipped, 13, previous: 6), 14)
    }

    func testLegalCaretPositionsAreNeverMoved() {
        let chipped = decorate("Fixes #EXP-42 today").attributed
        for position in [0, 3, 6, 14] {
            XCTAssertEqual(snap(chipped, position, previous: 0), position, "position \(position)")
        }
    }

    func testAnUnresolvedTokenIsPlainTextForTheCaretToo() {
        let plain = decorate("Fixes #EXP-99 today").attributed
        XCTAssertEqual(snap(plain, 7, previous: 6), 7)
        XCTAssertEqual(snap(plain, 12, previous: 13), 12)
    }

    /// With no title there is no seam: the atom ends where the token does.
    func testATitlelessChipSnapsToItsTokenEnds() {
        let noTitles: (String) -> String? = { _ in nil }
        let chipped = decorate("Fixes #EXP-42 today", titles: noTitles).attributed
        XCTAssertEqual(chipped.string, "Fixes #EXP-42 today")
        XCTAssertEqual(snap(chipped, 7, previous: 6), 13)
        XCTAssertEqual(snap(chipped, 12, previous: 13), 6)
        XCTAssertEqual(snap(chipped, 13, previous: 6), 13)
    }

    /// The hidden `#` splits the token's color run — `longestEffectiveRange` on
    /// `.markdownIssueRef` must still report the start, or the caret would snap
    /// to the identifier instead of in front of the pill.
    func testAStatusChipSnapsPastItsHiddenHash() {
        let chipped = decorate("Fixes #EXP-42 today", statuses: statuses).attributed
        XCTAssertEqual(snap(chipped, 7, previous: 6), 14)
        XCTAssertEqual(snap(chipped, 12, previous: 13), 6)
        XCTAssertEqual(snap(chipped, 6, previous: 5), 6)
    }
}
