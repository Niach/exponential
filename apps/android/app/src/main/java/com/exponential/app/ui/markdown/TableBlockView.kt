package com.exponential.app.ui.markdown

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import com.exponential.app.ui.markdown.model.TableAlignment
import com.exponential.app.ui.markdown.model.TableCell
import com.exponential.app.ui.markdown.model.TableData

/**
 * GFM tables (EXP-726) — the read renderer and the editor's cell grid, both
 * laid out by the same [TableGridLayout] so a table looks identical whether you
 * are reading it or typing in it.
 *
 * There is deliberately NO manipulation UI on mobile: no add/delete/move row or
 * column, no long-press menu. Cells are editable, and that is all — inserting
 * and moving is a desktop-web / IDE affordance.
 */
@Composable
internal fun TableBlockView(table: TableData, issueRefs: IssueRefHandler?) {
    if (table.columnCount == 0) return
    TableScrollBox {
        TableGridLayout(columnCount = table.columnCount, rowCount = table.rows.size + 1) {
            for (col in 0 until table.columnCount) {
                TableReadCell(table.header.getOrNull(col), table.alignmentAt(col), header = true, issueRefs)
            }
            for (row in table.rows) {
                for (col in 0 until table.columnCount) {
                    TableReadCell(row.getOrNull(col), table.alignmentAt(col), header = false, issueRefs)
                }
            }
        }
    }
}

/**
 * The editable twin of [TableBlockView]: the same grid, with every cell a
 * [BlockTextField] keyed by its CELL id — [EditorModel.updateRun] routes those
 * straight through to the cell.
 */
@Composable
internal fun TableRowEditView(model: EditorModel, row: EditorRow.Table) {
    val table = row.table
    if (table.columnCount == 0) return
    TableScrollBox {
        TableGridLayout(columnCount = table.columnCount, rowCount = table.rows.size + 1) {
            for (col in 0 until table.columnCount) {
                val cell = table.header.getOrNull(col) ?: continue
                key(cell.id) { TableCellField(model, cell) }
            }
            for (bodyRow in table.rows) {
                for (col in 0 until table.columnCount) {
                    val cell = bodyRow.getOrNull(col) ?: continue
                    key(cell.id) { TableCellField(model, cell) }
                }
            }
        }
    }
}

/** A table wider than the surface scrolls sideways rather than squeezing its columns. */
@Composable
private fun TableScrollBox(content: @Composable () -> Unit) {
    Box(
        modifier = Modifier
            .padding(vertical = 4.dp)
            .horizontalScroll(rememberScrollState()),
    ) {
        content()
    }
}

@Composable
private fun TableReadCell(
    cell: TableCell?,
    alignment: TableAlignment,
    header: Boolean,
    issueRefs: IssueRefHandler?,
) {
    val mentions = LocalMentions.current
    val autolink = LocalMarkdownAutolink.current
    val inlineCode = LocalInlineCodeStyle.current
    val text = cell?.text.orEmpty()
    val marks = cell?.marks.orEmpty()
    ChipText(
        line = annotateLine(text, marks, issueRefs, mentions, autolink, inlineCode),
        style = MdStyle.body.copy(
            fontWeight = if (header) FontWeight.SemiBold else null,
            textAlign = alignment.toTextAlign(),
        ),
        modifier = Modifier.padding(
            horizontal = MdStyle.tableCellPadX,
            vertical = MdStyle.tableCellPadY,
        ),
    )
}

/**
 * One editable cell. The cell is projected onto a synthetic single-paragraph
 * [EditorRow.TextRun] carrying the CELL's id, so every path [BlockTextField]
 * already has — value re-seeding on a revision bump, focus, selection, chips —
 * works unchanged and the model recognises the id as a cell.
 */
@Composable
private fun TableCellField(model: EditorModel, cell: TableCell) {
    val run = remember(cell.id, cell.text, cell.marks) {
        EditorRow.TextRun(
            id = cell.id,
            text = cell.text,
            paragraphs = listOf(ParagraphAttrs.PLAIN),
            marks = cell.marks,
        )
    }
    BlockTextField(
        model = model,
        row = run,
        placeholder = null,
        singleLine = true,
        modifier = Modifier.padding(
            horizontal = MdStyle.tableCellPadX,
            vertical = MdStyle.tableCellPadY,
        ),
    )
}

private fun TableAlignment.toTextAlign(): TextAlign = when (this) {
    TableAlignment.Left -> TextAlign.Start
    TableAlignment.Center -> TextAlign.Center
    TableAlignment.Right -> TextAlign.End
    TableAlignment.None -> TextAlign.Unspecified
}

/** The measured grid, handed from [TableGridLayout]'s layout pass to its painter. */
internal data class TableGeometry(
    val columnWidths: List<Int> = emptyList(),
    val rowHeights: List<Int> = emptyList(),
)

/**
 * A plain row-major grid: children arrive header row first, `columnCount` per
 * row. A column is as wide as its widest cell's intrinsic width, floored at
 * [MdStyle.tableCellMinWidth] and capped at [MdStyle.tableCellMaxWidth] so one
 * essay-length cell cannot push every other column out of reach; a row is as
 * tall as its tallest cell. The hairline grid and the header tint are painted
 * behind the cells from the measured geometry, which is why an editable cell
 * (whose own height grows as it wraps) still gets a continuous border.
 */
@Composable
internal fun TableGridLayout(
    columnCount: Int,
    rowCount: Int,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val geometry = remember { mutableStateOf(TableGeometry()) }
    val density = LocalDensity.current
    val minCellWidth = with(density) { MdStyle.tableCellMinWidth.roundToPx() }
    val maxCellWidth = with(density) { MdStyle.tableCellMaxWidth.roundToPx() }
    val stroke = with(density) { MdStyle.tableBorderWidth.toPx() }
    Layout(
        content = content,
        modifier = modifier.drawBehind { drawTableGrid(geometry.value, stroke) },
    ) { measurables, _ ->
        if (columnCount <= 0 || rowCount <= 0 || measurables.isEmpty()) {
            return@Layout layout(0, 0) {}
        }
        val widths = IntArray(columnCount) { minCellWidth }
        measurables.forEachIndexed { index, measurable ->
            val col = index % columnCount
            val natural = measurable.maxIntrinsicWidth(Constraints.Infinity)
                .coerceIn(minCellWidth, maxCellWidth)
            if (natural > widths[col]) widths[col] = natural
        }
        val placeables = measurables.mapIndexed { index, measurable ->
            val width = widths[index % columnCount]
            measurable.measure(Constraints(minWidth = width, maxWidth = width))
        }
        val heights = IntArray(rowCount)
        placeables.forEachIndexed { index, placeable ->
            val row = index / columnCount
            if (row < rowCount && placeable.height > heights[row]) heights[row] = placeable.height
        }
        // Written in the layout phase, read in the draw phase — the painter
        // needs the column/row runs and nothing else does.
        geometry.value = TableGeometry(widths.toList(), heights.toList())
        layout(widths.sum(), heights.sum()) {
            var y = 0
            for (row in 0 until rowCount) {
                var x = 0
                for (col in 0 until columnCount) {
                    placeables.getOrNull(row * columnCount + col)?.place(x, y)
                    x += widths[col]
                }
                y += heights[row]
            }
        }
    }
}

private fun DrawScope.drawTableGrid(geometry: TableGeometry, stroke: Float) {
    val widths = geometry.columnWidths
    val heights = geometry.rowHeights
    if (widths.isEmpty() || heights.isEmpty()) return
    val totalWidth = size.width
    val totalHeight = size.height
    drawRect(
        color = MdStyle.TableHeaderBg,
        topLeft = Offset.Zero,
        size = Size(totalWidth, heights.first().toFloat().coerceAtMost(totalHeight)),
    )
    // Rules rather than lines: a 1px stroke centred on the boundary would lose
    // its outer half at the table's own edges.
    var x = 0f
    for (i in 0..widths.size) {
        val left = if (i == widths.size) (totalWidth - stroke).coerceAtLeast(0f) else x
        drawRect(MdStyle.TableBorder, topLeft = Offset(left, 0f), size = Size(stroke, totalHeight))
        if (i < widths.size) x += widths[i]
    }
    var y = 0f
    for (i in 0..heights.size) {
        val top = if (i == heights.size) (totalHeight - stroke).coerceAtLeast(0f) else y
        drawRect(MdStyle.TableBorder, topLeft = Offset(0f, top), size = Size(totalWidth, stroke))
        if (i < heights.size) y += heights[i]
    }
}
