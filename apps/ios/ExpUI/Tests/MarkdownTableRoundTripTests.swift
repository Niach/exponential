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

    // MARK: - Nested tables hoist (EXP-728)

    /// Nested tables are NOT supported. A table is a top-level block; one
    /// found inside a list item or blockquote is hoisted out on parse (the
    /// list splits around it, the quote ends before it) and the hoisted form
    /// is the canonical one. `(name, nested, hoisted)` — every native flat
    /// model turns `nested` into `hoisted`; `hoisted` is a fixpoint on all
    /// four clients. Byte-mirrored by the desktop `TABLE_HOIST_FIXTURES`,
    /// Android's `MarkdownRoundTripTest.kt` and the web `markdown-table.test.ts`
    /// — add a fixture in all four or in none. Ordered lists stay out on
    /// purpose: how the tail item is renumbered after a hoist is
    /// engine-specific and not part of the contract.
    private static let hoistFixtures: [(name: String, nested: String, hoisted: String)] = [
        (
            "table_hoisted_from_list",
            "- step one\n\n  | a | b |\n  | --- | --- |\n  | 1 | 2 |\n\n- step two",
            "- step one\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n- step two"
        ),
        (
            "table_hoisted_from_quote",
            "> intro\n>\n> | a | b |\n> | --- | --- |\n> | 1 | 2 |",
            "> intro\n\n| a | b |\n| --- | --- |\n| 1 | 2 |"
        ),
    ]

    func testNestedTablesHoistToTheTopLevel() {
        for fixture in Self.hoistFixtures {
            XCTAssertEqual(
                roundTrip(fixture.nested), fixture.hoisted,
                "\(fixture.name) must hoist to the top level"
            )
            XCTAssertEqual(
                roundTrip(fixture.hoisted), fixture.hoisted,
                "\(fixture.name) hoisted form must be a fixpoint"
            )
        }
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

    /// A BACKSLASH standing in front of a pipe is data too.
    ///
    /// GFM strips the backslash from every `\|` pair before the inline parse
    /// (cmark-gfm's `unescape_pipes`), and the inline parser then folds a `\\`
    /// pair into one literal backslash. So a cell holding `a\|b` may not be
    /// written `a\\|b` — that unescapes to `a\` plus a cell SEPARATOR and the
    /// pipe is gone. The backslash run in front of a pipe is doubled first:
    /// `a\|b` → `a\\\|b`.
    func testABackslashBeforeAPipeSurvivesTheRoundTrip() {
        let src = "| a\\\\\\|b | c |\n| --- | --- |\n| 1 | 2 |"
        guard let table = firstTable(src) else { return XCTFail("expected a table block") }
        XCTAssertEqual(table.header[0].content.string, "a\\|b")
        XCTAssertEqual(roundTrip(src), src)
    }

    /// (parse, serialize) is a FIXPOINT for every backslash/pipe shape: what a
    /// cell holds is exactly what comes back out of its own serialization.
    func testEveryBackslashPipeShapeIsAParseSerializeFixpoint() {
        let cells = [
            "a|b",        // a|b
            "a\\|b",      // a\|b
            "a\\\\|b",    // a\\|b
            "|",          // |
            "\\|\\|",     // \|\|
            "a\\b",       // a\b  — not before a pipe, so untouched
        ]
        for text in cells {
            let table = TableBlock(header: [TableCell(content: NSAttributedString(string: text))])
            let markdown = MarkdownConversion.blocksToMarkdown([.table(id: UUID(), table: table)])
            guard let reparsed = firstTable(markdown) else {
                return XCTFail("expected a table block from \(markdown)")
            }
            XCTAssertEqual(
                reparsed.header[0].content.string, text,
                "cell text must survive its own serialization (\(markdown))")
            XCTAssertEqual(
                MarkdownConversion.blocksToMarkdown([.table(id: UUID(), table: reparsed)]),
                markdown,
                "a second pass must not move a byte")
        }
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
