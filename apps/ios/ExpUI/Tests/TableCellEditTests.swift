import Foundation
import UIKit
import XCTest
import ExpUI

/// EXP-726 — cell editing is the ONLY table affordance mobile ships: no
/// add/delete/move row or column, no long-press menus. These lock the model
/// routing that makes a cell behave like any other block (`updateText` by id,
/// revisions, focus) while the derived markdown stays canonical.
@MainActor
final class TableCellEditTests: XCTestCase {
    private static let source = "| a | b |\n| --- | --- |\n| 1 | 2 |"

    private func loadedModel(_ markdown: String = source) -> IssueEditorModel {
        let model = IssueEditorModel()
        model.load(markdown: markdown, baseURL: nil)
        return model
    }

    private func table(_ model: IssueEditorModel) -> (id: UUID, table: TableBlock)? {
        for block in model.blocks {
            if case let .table(id, table) = block { return (id, table) }
        }
        return nil
    }

    private func text(_ string: String) -> NSAttributedString {
        NSAttributedString(string: string, attributes: MarkdownStyle.baseAttributes)
    }

    func testALoadedTableIsNotDirty() {
        let model = loadedModel()
        XCTAssertEqual(model.currentMarkdown(), Self.source)
        XCTAssertFalse(model.isDirty)
    }

    func testUpdateTableCellRewritesOneCellAndMarksTheModelDirty() {
        let model = loadedModel()
        guard let found = table(model) else { return XCTFail("expected a table block") }
        model.updateTableCell(blockId: found.id, row: 1, col: 0, content: text("changed"))
        XCTAssertEqual(
            model.currentMarkdown(),
            "| a | b |\n| --- | --- |\n| changed | 2 |"
        )
        XCTAssertTrue(model.isDirty)
    }

    func testTheHeaderRowIsRow0() {
        let model = loadedModel()
        guard let found = table(model) else { return XCTFail("expected a table block") }
        model.updateTableCell(blockId: found.id, row: 0, col: 1, content: text("head"))
        XCTAssertEqual(model.currentMarkdown(), "| a | head |\n| --- | --- |\n| 1 | 2 |")
    }

    func testUpdateTextRoutesACellIdToItsCell() {
        // The text views know nothing about tables: they call `updateText(id:)`
        // with the CELL's id, which shares the block-id namespace.
        let model = loadedModel()
        guard let found = table(model) else { return XCTFail("expected a table block") }
        let cellId = found.table.rows[0][1].id
        model.updateText(id: cellId, content: text("nine"))
        XCTAssertEqual(model.currentMarkdown(), "| a | b |\n| --- | --- |\n| 1 | nine |")
    }

    func testANewlineWrittenIntoACellCollapsesToASpace() {
        let model = loadedModel()
        guard let found = table(model) else { return XCTFail("expected a table block") }
        model.updateTableCell(blockId: found.id, row: 1, col: 0, content: text("one\ntwo"))
        XCTAssertEqual(model.currentMarkdown(), "| a | b |\n| --- | --- |\n| one two | 2 |")
    }

    func testAPipeTypedIntoACellIsEscapedOnSave() {
        let model = loadedModel()
        guard let found = table(model) else { return XCTFail("expected a table block") }
        model.updateTableCell(blockId: found.id, row: 1, col: 1, content: text("x | y"))
        XCTAssertEqual(model.currentMarkdown(), "| a | b |\n| --- | --- |\n| 1 | x \\| y |")
    }

    func testAnEmptiedCellSerializesAsTheTwoSpaceForm() {
        let model = loadedModel()
        guard let found = table(model) else { return XCTFail("expected a table block") }
        model.updateTableCell(blockId: found.id, row: 1, col: 1, content: NSAttributedString())
        XCTAssertEqual(model.currentMarkdown(), "| a | b |\n| --- | --- |\n| 1 |  |")
    }

    func testACellEditFiresTheHostEditHook() {
        let model = loadedModel()
        guard let found = table(model) else { return XCTFail("expected a table block") }
        var edits = 0
        model.onEdit = { edits += 1 }
        model.updateTableCell(blockId: found.id, row: 0, col: 0, content: text("z"))
        XCTAssertEqual(edits, 1)
    }

    func testTheFocusedIdMayBeACellId() {
        let model = loadedModel()
        guard let found = table(model) else { return XCTFail("expected a table block") }
        let cellId = found.table.header[0].id
        model.setFocused(cellId)
        XCTAssertEqual(model.focusedBlockId, cellId)
        XCTAssertTrue(model.isEditing)
        model.clearFocusIfMatches(cellId)
        XCTAssertNil(model.focusedBlockId)
    }

    func testFocusCellAfterWalksRowMajorAndStopsAtTheLastCell() {
        let model = loadedModel()
        guard let found = table(model) else { return XCTFail("expected a table block") }
        let cells = found.table.allCells.map(\.id)
        XCTAssertEqual(cells.count, 4)

        model.setFocused(cells[0])
        model.focusCell(after: cells[0])
        XCTAssertEqual(model.focusedBlockId, cells[1], "header cell 0 → header cell 1")
        model.focusCell(after: cells[1])
        XCTAssertEqual(model.focusedBlockId, cells[2], "header wraps into the first body row")
        model.focusCell(after: cells[2])
        XCTAssertEqual(model.focusedBlockId, cells[3])
        // The last cell stays put: Return must never grow the table on mobile.
        model.focusCell(after: cells[3])
        XCTAssertEqual(model.focusedBlockId, cells[3])
    }

    func testFocusCellAfterPutsTheCaretAtTheEndOfTheNextCell() {
        let model = loadedModel("| abcd | b |\n| --- | --- |\n| 1 | 2 |")
        guard let found = table(model) else { return XCTFail("expected a table block") }
        let cells = found.table.allCells.map(\.id)
        model.focusCell(after: cells[0])
        XCTAssertEqual(model.consumeDesiredSelection(for: cells[1]), 1)
    }

    func testACellEditSurvivesAReloadOfTheDerivedMarkdown() {
        let model = loadedModel()
        guard let found = table(model) else { return XCTFail("expected a table block") }
        model.updateTableCell(blockId: found.id, row: 1, col: 0, content: text("edited"))
        let saved = model.currentMarkdown()

        let reloaded = IssueEditorModel()
        reloaded.load(markdown: saved, baseURL: nil)
        XCTAssertEqual(reloaded.currentMarkdown(), saved)
        XCTAssertFalse(reloaded.isDirty)
    }

    // MARK: - The bottom bar's insertion affordances inside a cell

    /// The emoji picker inserts UNICODE through `insertTextAtCaret`. With the
    /// caret in a cell that used to be a silent no-op (the guard only accepted
    /// a `.text` block), so the tapped emoji simply vanished.
    func testAnEmojiInsertLandsAtTheCaretInsideTheFocusedCell() {
        let model = loadedModel("| ab | b |\n| --- | --- |\n| 1 | 2 |")
        guard let found = table(model) else { return XCTFail("expected a table block") }
        let cellId = found.table.header[0].id
        model.setFocused(cellId)
        model.updateSelection(blockId: cellId, range: NSRange(location: 1, length: 0))
        model.insertTextAtCaret("\u{1F604}")
        XCTAssertEqual(
            model.currentMarkdown(),
            "| a\u{1F604}b | b |\n| --- | --- |\n| 1 | 2 |")
        // The caret sits after the inserted emoji, measured in UTF-16.
        XCTAssertEqual(model.consumeDesiredSelection(for: cellId), 3)
    }

    /// A newline can never reach a cell: `writeCell` folds it to a space, so
    /// even a pasted multi-line insertion stays one row.
    func testAnInsertionWithANewlineFoldsToASpaceInsideACell() {
        let model = loadedModel()
        guard let found = table(model) else { return XCTFail("expected a table block") }
        let cellId = found.table.rows[0][0].id
        model.setFocused(cellId)
        model.updateSelection(blockId: cellId, range: NSRange(location: 1, length: 0))
        model.insertTextAtCaret("x\ny")
        XCTAssertEqual(model.currentMarkdown(), "| a | b |\n| --- | --- |\n| 1x y | 2 |")
    }

    /// `@` completes inside a cell: the recompute reads the cell's caret
    /// context and `applyMention` writes the canonical plain `@email ` token
    /// back into the cell, not into some text block.
    func testAMentionCompletesInsideACell() {
        let model = loadedModel()
        model.mentionMembers = [MentionMember(name: "Ada Lovelace", email: "ada@example.com")]
        guard let found = table(model) else { return XCTFail("expected a table block") }
        let cellId = found.table.rows[0][1].id

        // Replays the text view's callback order: the keystroke, then the
        // settled caret (only a text change may OPEN the bar, EXP-322).
        model.setFocused(cellId)
        model.updateText(id: cellId, content: text("@ad"))
        model.updateSelection(blockId: cellId, range: NSRange(location: 3, length: 0))
        XCTAssertEqual(model.mentionCandidates.map(\.email), ["ada@example.com"])

        model.applyMention(model.mentionCandidates[0])
        XCTAssertEqual(
            model.currentMarkdown(),
            "| a | b |\n| --- | --- |\n| 1 | @ada@example.com |")
        XCTAssertTrue(model.mentionCandidates.isEmpty)
    }

    /// The `:shortcode` typeahead completes inside a cell too, through the same
    /// recompute.
    func testAnEmojiShortcodeCompletesInsideACell() {
        let model = loadedModel()
        let smile = EmojiRecord(
            unicode: "\u{1F604}", label: "smile", group: 0,
            shortcodes: ["smile"], tags: []
        )
        model.emojiSearch = { $0.isEmpty || "smile".hasPrefix($0) ? [smile] : [] }
        guard let found = table(model) else { return XCTFail("expected a table block") }
        let cellId = found.table.header[1].id

        model.setFocused(cellId)
        model.updateText(id: cellId, content: text(":sm"))
        model.updateSelection(blockId: cellId, range: NSRange(location: 3, length: 0))
        XCTAssertEqual(model.emojiCandidates.map(\.unicode), ["\u{1F604}"])

        model.applyEmoji(model.emojiCandidates[0])
        XCTAssertEqual(
            model.currentMarkdown(),
            "| a | \u{1F604} |\n| --- | --- |\n| 1 | 2 |")
    }

    /// `#` completes inside a cell, and the chip decoration it triggers stays
    /// serialization-invisible there (see `IssueRefChipTests`).
    func testAnIssueRefCompletesInsideACell() {
        let model = loadedModel()
        model.issueRefSearch = { _ in
            [IssueRefCandidate(identifier: "EXP-42", title: "Tables")]
        }
        guard let found = table(model) else { return XCTFail("expected a table block") }
        let cellId = found.table.rows[0][0].id

        model.setFocused(cellId)
        model.updateText(id: cellId, content: text("#EX"))
        model.updateSelection(blockId: cellId, range: NSRange(location: 3, length: 0))
        XCTAssertEqual(model.issueRefCandidates.map(\.identifier), ["EXP-42"])

        model.applyIssueRef(model.issueRefCandidates[0])
        XCTAssertEqual(
            model.currentMarkdown(),
            "| a | b |\n| --- | --- |\n| #EXP-42 | 2 |")
    }

    /// An image cannot live inside a cell (one inline paragraph), so it lands
    /// immediately AFTER the table rather than at the end of the document —
    /// which is where the old `.text`-only guard dropped it.
    func testAnImageInsertedFromACellLandsRightAfterTheTable() {
        let model = loadedModel("intro\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\noutro")
        guard let found = table(model) else { return XCTFail("expected a table block") }
        let tableIndex = model.blocks.firstIndex { $0.id == found.id }
        model.setFocused(found.table.rows[0][0].id)
        model.insertImage(
            data: Data([0x1]), filename: "shot.png", contentType: "image/png",
            width: 10, height: 10)

        let kinds = model.blocks.map { block -> String in
            switch block {
            case .text: return "text"
            case .image: return "image"
            case .table: return "table"
            }
        }
        // `normalize` pads the table/image pair with the empty text block the
        // caret needs; the trailing paragraph is untouched.
        XCTAssertEqual(kinds, ["text", "table", "text", "image", "text", "text"])
        XCTAssertEqual(model.blocks.firstIndex { $0.id == found.id }, tableIndex)
        // The trailing paragraph is still the LAST block: the image did not
        // fall through to the append-at-the-end path.
        guard case let .text(_, last) = model.blocks[5] else {
            return XCTFail("expected a trailing text block")
        }
        XCTAssertEqual(last.string, "outro")
        XCTAssertTrue(model.currentMarkdown().contains(
            "| 1 | 2 |\n\n![image]"))
    }

    func testAnUnknownIdIsANoOp() {
        let model = loadedModel()
        model.updateText(id: UUID(), content: text("nowhere"))
        model.focusCell(after: UUID())
        XCTAssertEqual(model.currentMarkdown(), Self.source)
    }
}
