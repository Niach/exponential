import Foundation

/// GFM pipe tables (EXP-726), the model half. The wire contract is shared by
/// all four clients and byte-locked by `MarkdownTableRoundTripTests` here, the
/// desktop `CONTRACT_FIXTURES`, `MarkdownRoundTripTest.kt` and the web
/// round-trip suite:
///
/// ```
/// | a | b |
/// | --- | --- |
/// | 1 | 2 |
/// ```
///
/// One space each side of the cell text, no column-width padding, rows joined
/// by `\n`, a blank line before/after (that falls out of the block join).
/// Delimiter cells are `---` / `:---` / `:---:` / `---:`, an empty cell is
/// `|  |`, `|` inside a cell is written `\|` and a newline inside a cell
/// becomes a space. Row 0 is ALWAYS the header row; a header-only table is
/// valid. Cells are ONE inline paragraph, so an image inside a cell stays
/// literal `![alt](url)` text.

/// Per-column alignment, from the delimiter row's colons.
public enum TableAlignment: String, Sendable, Equatable, CaseIterable {
    case none
    case left
    case center
    case right

    /// The delimiter-row cell this alignment serializes to.
    public var delimiter: String {
        switch self {
        case .none: return "---"
        case .left: return ":---"
        case .center: return ":---:"
        case .right: return "---:"
        }
    }
}

/// One table cell. Its `id` lives in the SAME namespace as `ContentBlock.id`,
/// so `IssueEditorModel`'s routing (`updateText`, `applyDecoration`,
/// `revision(for:)`, focus) addresses a cell exactly like a block.
public struct TableCell: Identifiable, Equatable {
    public let id: UUID
    public var content: NSAttributedString

    public init(id: UUID = UUID(), content: NSAttributedString = NSAttributedString()) {
        self.id = id
        self.content = content
    }
}

/// A parsed pipe table. `header` is row 0; `rows` are the body rows, each
/// padded/truncated to `columnCount` on parse so the grid is never ragged.
public struct TableBlock: Equatable {
    public var header: [TableCell]
    public var rows: [[TableCell]]
    public var alignments: [TableAlignment]

    public init(
        header: [TableCell],
        rows: [[TableCell]] = [],
        alignments: [TableAlignment] = []
    ) {
        let columns = header.count
        self.header = header
        self.rows = rows.map { row in
            var padded = Array(row.prefix(columns))
            while padded.count < columns { padded.append(TableCell()) }
            return padded
        }
        var alignment = Array(alignments.prefix(columns))
        while alignment.count < columns { alignment.append(.none) }
        self.alignments = alignment
    }

    public var columnCount: Int { header.count }

    /// Header + body. Row 0 is the header, body row `i` is row `i + 1`.
    public var rowCount: Int { 1 + rows.count }

    public func alignment(column: Int) -> TableAlignment {
        column >= 0 && column < alignments.count ? alignments[column] : .none
    }

    public func cell(row: Int, col: Int) -> TableCell? {
        guard col >= 0, col < columnCount else { return nil }
        if row == 0 { return header[col] }
        let body = row - 1
        guard body >= 0, body < rows.count else { return nil }
        return rows[body][col]
    }

    /// Grid position of `id`, row 0 being the header. Nil when the id belongs
    /// to another table (or to a block).
    public func cell(id: UUID) -> (row: Int, col: Int)? {
        if let col = header.firstIndex(where: { $0.id == id }) { return (0, col) }
        for (index, row) in rows.enumerated() {
            if let col = row.firstIndex(where: { $0.id == id }) { return (index + 1, col) }
        }
        return nil
    }

    /// Every cell in ROW-MAJOR order, header first — the order Return walks.
    public var allCells: [TableCell] {
        var cells = header
        for row in rows { cells.append(contentsOf: row) }
        return cells
    }

    /// The next cell in row-major order, or nil at the last cell.
    public func cellId(after id: UUID) -> UUID? {
        let cells = allCells
        guard let index = cells.firstIndex(where: { $0.id == id }),
              index + 1 < cells.count else { return nil }
        return cells[index + 1].id
    }

    public mutating func setContent(row: Int, col: Int, content: NSAttributedString) {
        guard col >= 0, col < columnCount else { return }
        if row == 0 {
            header[col].content = content
            return
        }
        let body = row - 1
        guard body >= 0, body < rows.count else { return }
        rows[body][col].content = content
    }

    /// Rewrite every cell through `transform` (nil = leave it alone) and
    /// return the ids that actually changed — the shape `decorateChips` and
    /// `redecorateChips` need.
    public mutating func transformCells(
        _ transform: (TableCell) -> NSAttributedString?
    ) -> [UUID] {
        var changed: [UUID] = []
        for col in header.indices {
            if let next = transform(header[col]) {
                header[col].content = next
                changed.append(header[col].id)
            }
        }
        for row in rows.indices {
            for col in rows[row].indices {
                if let next = transform(rows[row][col]) {
                    rows[row][col].content = next
                    changed.append(rows[row][col].id)
                }
            }
        }
        return changed
    }
}
