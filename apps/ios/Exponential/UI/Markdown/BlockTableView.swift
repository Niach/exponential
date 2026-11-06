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
/// A read-only editor (`MarkdownEditor(isReadOnly:)`) gets the same grid
/// with non-editable cells; read-only Markdown is genUI's (#VAPP-258).
struct BlockTableView: View {
    let model: IssueEditorModel
    let blockId: UUID
    let table: TableBlock
    var isReadOnly = false
    var onIssueRefTap: ((String) -> Void)?
    /// The scroller's own width: a table narrower than it fills it (web
    /// `table { width: 100% }`, EXP-1676), a wider one scrolls.
    @State private var viewport: CGFloat = 0

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            TableFillLayout(viewport: viewport) {
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
            // EXP-1362: every cell stretches to its row (`maxHeight:
            // .infinity`), so a Grid PROPOSED more height than it needs
            // spreads the surplus over every row — a one-line header row
            // came out three lines tall. Pin the grid to its ideal height.
            .fixedSize(horizontal: false, vertical: true)
            .overlay {
                // Square ×4 (web `border-collapse`, Android's grid).
                Rectangle().strokeBorder(Color(MarkdownStyle.tableBorder), lineWidth: 1)
            }
            }
            .padding(.vertical, 4)
            .padding(.horizontal, 1)
        }
        .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { viewport = max(0, $0 - 2) }
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
            content: isHeader ? Self.headerContent(cell.content) : cell.content,
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
            cellPadding: padding,
            onReturn: { model.focusCell(after: cell.id) },
            onDeleteTable: isReadOnly ? nil : { model.deleteTableBlock(id: blockId) },
            onIssueRefTap: onIssueRefTap
        )
        .frame(
            // A column never wraps below its content while the table fills
            // (CSS `table-layout: auto`); a long cell still wraps at the cap.
            minWidth: min(
                max(MarkdownStyle.tableCellMinWidth, ceil(cell.content.size().width) + padding.width * 2 + 0.8),
                MarkdownStyle.tableCellMaxWidth
            ),
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

    private var padding: CGSize {
        CGSize(width: MarkdownStyle.tableCellPadX, height: MarkdownStyle.tableCellPadY)
    }

    /// Web's `th` weight, display only (`MarkdownStyle.tableHeaderStroke`).
    private static func headerContent(_ content: NSAttributedString) -> NSAttributedString {
        let bold = NSMutableAttributedString(attributedString: content)
        bold.addAttribute(
            .strokeWidth,
            value: MarkdownStyle.tableHeaderStroke,
            range: NSRange(location: 0, length: bold.length)
        )
        return bold
    }

    private static func alignment(_ alignment: TableAlignment) -> NSTextAlignment {
        switch alignment {
        case .none, .left: return .natural
        case .center: return .center
        case .right: return .right
        }
    }
}

/// EXP-1676: proposes the grid the wider of its ideal width and the viewport,
/// inside the horizontal scroller (whose nil proposal alone hugs the content).
private struct TableFillLayout: Layout {
    let viewport: CGFloat

    private func width(_ subview: LayoutSubview) -> CGFloat {
        max(subview.sizeThatFits(.unspecified).width, viewport)
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let grid = subviews.first else { return .zero }
        let width = width(grid)
        let size = grid.sizeThatFits(ProposedViewSize(width: width, height: nil))
        return CGSize(width: width, height: size.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard let grid = subviews.first else { return }
        grid.place(at: bounds.origin, proposal: ProposedViewSize(width: bounds.width, height: bounds.height))
    }
}
