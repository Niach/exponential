import ExpUI
import SwiftUI
import UIKit

/// EXP-726 — a GFM pipe table, rendered as a horizontally scrollable grid of
/// editable cells.
///
/// Mobile deliberately ships NO structural manipulation UI: no add/delete/move
/// row or column. Cells are editable, and the ONE table action is "Delete
/// table" on a cell's long-press edit menu (EXP-727) — row and column
/// structure is authored on desktop web or in the IDE. Return moves to the
/// next cell (row-major, header first) rather than growing the table.
///
/// Read-only surfaces (comment cards, the description preview, the steer feed's
/// `AgentMarkdownText`, actions) route through `MarkdownEditor(isReadOnly:)`
/// and get the same grid with non-editable cells.
struct BlockTableView: View {
    let model: IssueEditorModel
    let blockId: UUID
    let table: TableBlock
    var isReadOnly = false
    var onIssueRefTap: ((String) -> Void)?

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            Grid(alignment: .topLeading, horizontalSpacing: 0, verticalSpacing: 0) {
                GridRow {
                    ForEach(Array(table.header.enumerated()), id: \.element.id) { column, cell in
                        cellView(cell, row: 0, col: column, isHeader: true)
                    }
                }
                ForEach(Array(table.rows.enumerated()), id: \.offset) { index, row in
                    GridRow {
                        ForEach(Array(row.enumerated()), id: \.element.id) { column, cell in
                            cellView(cell, row: index + 1, col: column, isHeader: false)
                        }
                    }
                }
            }
            .overlay {
                RoundedRectangle(cornerRadius: 6)
                    .strokeBorder(Color(MarkdownStyle.tableBorder), lineWidth: 1)
            }
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .padding(.vertical, 4)
            .padding(.horizontal, 1)
        }
        // The grid owns the horizontal overflow; the enclosing column must not
        // be widened by a wide table.
        .scrollBounceBehavior(.basedOnSize)
    }

    private func cellView(
        _ cell: TableCell,
        row: Int,
        col: Int,
        isHeader: Bool
    ) -> some View {
        BlockTextEditor(
            model: model,
            blockId: cell.id,
            content: cell.content,
            revision: model.revision(for: cell.id),
            isFocused: model.focusedBlockId == cell.id,
            placeholder: nil,
            // No formatting accessory strip inside a cell: its block-level
            // actions (headings, lists, quotes, fences) have no meaning in one
            // inline paragraph.
            toolbar: nil,
            isReadOnly: isReadOnly,
            singleLine: true,
            textAlignment: Self.alignment(table.alignment(column: col)),
            onReturn: { model.focusCell(after: cell.id) },
            onDeleteTable: isReadOnly ? nil : { model.deleteTableBlock(id: blockId) },
            onIssueRefTap: onIssueRefTap
        )
        .frame(
            minWidth: MarkdownStyle.tableCellMinWidth,
            maxWidth: MarkdownStyle.tableCellMaxWidth,
            alignment: .topLeading
        )
        .frame(maxHeight: .infinity, alignment: .topLeading)
        .background(isHeader ? Color(MarkdownStyle.tableHeaderBackground) : Color.clear)
        .overlay {
            Rectangle()
                .strokeBorder(Color(MarkdownStyle.tableBorder), lineWidth: 0.5)
        }
    }

    private static func alignment(_ alignment: TableAlignment) -> NSTextAlignment {
        switch alignment {
        case .none, .left: return .natural
        case .center: return .center
        case .right: return .right
        }
    }
}
