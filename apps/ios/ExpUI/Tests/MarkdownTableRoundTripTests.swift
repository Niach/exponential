import Foundation
import XCTest
import ExpUI

/// EXP-726 — the shared GFM table wire contract, byte-locked.
///
/// Every fixture here is mirrored verbatim in the desktop `CONTRACT_FIXTURES`
/// (`apps/desktop/crates/ui/src/markdown/serialize.rs`), Android's
/// `MarkdownRoundTripTest.kt` and the web round-trip suite. A table authored on
/// any client must survive an edit cycle on every other one WITHOUT a byte
/// moving, so these strings are the contract, not examples of it:
///
/// ```
/// | a | b |
/// | --- | --- |
/// | 1 | 2 |
/// ```
///
/// One space each side of the cell text, no column-width padding, rows joined
/// by `\n`, a blank line around the table. Delimiter cells `---` / `:---` /
/// `:---:` / `---:`, empty cell `|  |`, `|` inside a cell written `\|`.
final class MarkdownTableRoundTripTests: XCTestCase {
    private func roundTrip(_ markdown: String) -> String {
        MarkdownConversion.blocksToMarkdown(MarkdownConversion.markdownToBlocks(markdown))
    }

    // MARK: - Shared fixtures (byte equality)

    /// Name → canonical source. Keep byte-identical to the other three clients.
    private static let fixtures: [(name: String, markdown: String)] = [
        ("table_basic", "| a | b |\n| --- | --- |\n| 1 | 2 |"),
        ("table_alignments", "| l | c | r | n |\n| :--- | :---: | ---: | --- |\n| 1 | 2 | 3 | 4 |"),
        (
            "table_inline_marks",
            "| **bold** | [link](https://example.com) |\n| --- | --- |\n| `code` | *em* |"
        ),
        ("table_escaped_pipe", "| a \\| b | c |\n| --- | --- |\n| 1 | 2 |"),
        ("table_empty_cell", "| a | b |\n| --- | --- |\n| 1 |  |"),
        ("table_header_only", "| a | b |\n| --- | --- |"),
        (
            "table_between_paragraphs",
            "before\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nafter"
        ),
        ("table_chip_cells", "| @jane@example.com | #EXP-42 |\n| --- | --- |\n| x | y |"),
        ("table_unicode", "| Grüße | 🚀 |\n| --- | --- |\n| ü | é |"),
    ]

    func testEveryContractFixtureRoundTripsByteForByte() {
        for fixture in Self.fixtures {
            XCTAssertEqual(
                roundTrip(fixture.markdown), fixture.markdown,
                "\(fixture.name) must round-trip byte-for-byte"
            )
        }
    }

    func testEveryContractFixtureIsAFixpoint() {
        // A second cycle must not drift either — the editor autosaves, so a
        // one-byte-per-save creep is the failure mode this catches.
        for fixture in Self.fixtures {
            let once = roundTrip(fixture.markdown)
            XCTAssertEqual(roundTrip(once), once, "\(fixture.name) must be stable")
        }
    }

    // MARK: - Normalisation

    func testGitHubStylePaddedInputNormalisesToTheCanonicalForm() {
        let padded = """
        | Name  | Value |
        | ----- | ----- |
        | alpha | 1     |
        | beta  | 2     |
        """
        let canonical = "| Name | Value |\n| --- | --- |\n| alpha | 1 |\n| beta | 2 |"
        XCTAssertEqual(roundTrip(padded), canonical)
        XCTAssertEqual(roundTrip(canonical), canonical)
    }

    func testPaddedAlignmentDelimitersNormalise() {
        let padded = "| l    | c     | r    |\n| :--- | :---: | ---: |\n| 1    | 2     | 3    |"
        XCTAssertEqual(
            roundTrip(padded),
            "| l | c | r |\n| :--- | :---: | ---: |\n| 1 | 2 | 3 |"
        )
    }

    func testRaggedBodyRowsArePaddedWithEmptyCells() {
        let ragged = "| a | b | c |\n| --- | --- | --- |\n| 1 |\n| 1 | 2 | 3 |"
        XCTAssertEqual(
            roundTrip(ragged),
            "| a | b | c |\n| --- | --- | --- |\n| 1 |  |  |\n| 1 | 2 | 3 |"
        )
    }

    func testExtraCellsBeyondTheHeaderWidthAreDropped() {
        // cmark itself truncates, and `TableBlock.init` re-asserts it — a body
        // row can never be wider than the header, whichever side clips.
        let wide = "| a | b |\n| --- | --- |\n| 1 | 2 | 3 |"
        XCTAssertEqual(roundTrip(wide), "| a | b |\n| --- | --- |\n| 1 | 2 |")
    }

    func testANewlineInsideACellBecomesASpace() {
        // A soft break inside a cell is GFM-legal only via `<br>`; cmark folds
        // the wrapped source into one inline run and the cell stays one line.
        let src = "| a | b |\n| --- | --- |\n| one two | 2 |"
        XCTAssertEqual(roundTrip(src), src)
    }

    // MARK: - Block structure

    private func blockKinds(_ markdown: String) -> [String] {
        MarkdownConversion.markdownToBlocks(markdown).map { block in
            switch block {
            case .text: return "text"
            case .image: return "image"
            case .table: return "table"
            }
        }
    }

    func testATableBetweenParagraphsParsesAsThreeBlocks() {
        XCTAssertEqual(
            blockKinds("before\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nafter"),
            ["text", "table", "text"]
        )
    }

    func testALoneTableIsPaddedWithTextBlocksLikeAnImage() {
        // `ContentBlock.normalize` keeps a text block above and below every
        // block-level element so the editor always has somewhere to type.
        XCTAssertEqual(blockKinds("| a | b |\n| --- | --- |"), ["text", "table", "text"])
    }

    private func firstTable(_ markdown: String) -> TableBlock? {
        for block in MarkdownConversion.markdownToBlocks(markdown) {
            if case let .table(_, table) = block { return table }
        }
        return nil
    }

    func testHeaderAndBodyCellsLandInTheRightPlaces() {
        guard let table = firstTable("| a | b |\n| --- | --- |\n| 1 | 2 |") else {
            return XCTFail("expected a table block")
        }
        XCTAssertEqual(table.columnCount, 2)
        XCTAssertEqual(table.rowCount, 2)
        XCTAssertEqual(table.header.map(\.content.string), ["a", "b"])
        XCTAssertEqual(table.rows.map { $0.map(\.content.string) }, [["1", "2"]])
        XCTAssertEqual(table.cell(row: 0, col: 1)?.content.string, "b")
        XCTAssertEqual(table.cell(row: 1, col: 0)?.content.string, "1")
    }

    func testAlignmentsComeOffTheDelimiterRow() {
        guard let table = firstTable(
            "| l | c | r | n |\n| :--- | :---: | ---: | --- |\n| 1 | 2 | 3 | 4 |"
        ) else { return XCTFail("expected a table block") }
        XCTAssertEqual(table.alignments, [.left, .center, .right, .none])
    }

    func testAHeaderOnlyTableHasNoBodyRows() {
        guard let table = firstTable("| a | b |\n| --- | --- |") else {
            return XCTFail("expected a table block")
        }
        XCTAssertTrue(table.rows.isEmpty)
        XCTAssertEqual(table.rowCount, 1)
    }

    func testAnEscapedPipeIsUnescapedInTheCellAndReEscapedOnSave() {
        guard let table = firstTable("| a \\| b | c |\n| --- | --- |\n| 1 | 2 |") else {
            return XCTFail("expected a table block")
        }
        XCTAssertEqual(table.header[0].content.string, "a | b")
        XCTAssertEqual(
            MarkdownConversion.blocksToMarkdown([.table(id: UUID(), table: table)]),
            "| a \\| b | c |\n| --- | --- |\n| 1 | 2 |"
        )
    }

    func testAnImageInsideACellStaysLiteralText() {
        // A cell is ONE inline paragraph on every client, so it can never host
        // an image BLOCK — the source rides through as text.
        let src = "| ![alt](/api/attachments/1) | b |\n| --- | --- |\n| 1 | 2 |"
        XCTAssertEqual(roundTrip(src), src)
        XCTAssertEqual(blockKinds(src), ["text", "table", "text"])
    }

    func testCellIdsAreUniqueAndAddressable() {
        guard let table = firstTable("| a | b |\n| --- | --- |\n| 1 | 2 |") else {
            return XCTFail("expected a table block")
        }
        let ids = table.allCells.map(\.id)
        XCTAssertEqual(ids.count, 4)
        XCTAssertEqual(Set(ids).count, 4, "cell ids share the block-id namespace and must be unique")
        XCTAssertEqual(table.cell(id: ids[3]).map { [$0.row, $0.col] }, [1, 1])
        // Row-major, header first.
        XCTAssertEqual(table.cellId(after: ids[1]), ids[2])
        XCTAssertNil(table.cellId(after: ids[3]))
    }
}
