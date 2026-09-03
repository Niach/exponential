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

    func testAnUnknownIdIsANoOp() {
        let model = loadedModel()
        model.updateText(id: UUID(), content: text("nowhere"))
        model.focusCell(after: UUID())
        XCTAssertEqual(model.currentMarkdown(), Self.source)
    }
}
