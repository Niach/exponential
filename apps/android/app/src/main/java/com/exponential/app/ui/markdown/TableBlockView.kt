package com.exponential.app.ui.markdown

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalTextToolbar
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import com.exponential.app.ui.markdown.model.TableAlignment
import com.exponential.app.ui.markdown.model.TableCell
import com.exponential.app.ui.markdown.model.TableData

/**
 * GFM tables (EXP-726): the editor's cell grid, laid out by [TableGridLayout]
 * (#VAPP-258: the read path is genUI's Markdown table, [MarkdownView]).
 *
 * There is deliberately NO structural manipulation UI on mobile: no add /
 * delete / move row or column. Cells are editable, and the ONE table action
 * is "Delete table" on a cell's long-press selection toolbar (EXP-727,
 * [TableCellTextToolbar]) — inserting and moving is a desktop-web / IDE
 * affordance.
 */
/**
 * The editor's table: the grid, with every cell a
 * [BlockTextField] keyed by its CELL id — [EditorModel.updateRun] routes those
 * straight through to the cell. Every cell's selection toolbar carries the
 * table's one action, Delete table (EXP-727).
 */
@Composable
internal fun TableRowEditView(model: EditorModel, row: EditorRow.Table) {
    val table = row.table
    if (table.columnCount == 0) return
    val toolbar = rememberTableCellTextToolbar { model.deleteTableRow(row.id) }
    TableScrollBox { viewport ->
        CompositionLocalProvider(LocalTextToolbar provides toolbar) {
            TableGridLayout(columnCount = table.columnCount, rowCount = table.rows.size + 1, viewport = viewport) {
                for (col in 0 until table.columnCount) {
                    val cell = table.header.getOrNull(col) ?: continue
                    key(cell.id) { TableCellField(model, cell, header = true) }
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
}

/**
 * A table wider than the surface scrolls sideways rather than squeezing its
 * columns; a narrower one fills it (web `table { width: 100% }`, EXP-1676).
 * The column's width reaches the grid as state, read in its measure pass (no
 * subcomposition: a table can sit in a row that asks intrinsics).
 */
@Composable
private fun TableScrollBox(content: @Composable (viewport: () -> Int) -> Unit) {
    val viewport = remember { mutableIntStateOf(0) }
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .layout { measurable, constraints ->
                if (constraints.hasBoundedWidth) viewport.intValue = constraints.maxWidth
                val placeable = measurable.measure(constraints)
                layout(placeable.width, placeable.height) { placeable.place(0, 0) }
            }
            .horizontalScroll(rememberScrollState()),
    ) {
        content { viewport.intValue }
    }
}

/**
 * One editable cell. The cell is projected onto a synthetic single-paragraph
 * [EditorRow.TextRun] carrying the CELL's id, so every path [BlockTextField]
 * already has — value re-seeding on a revision bump, focus, selection, chips —
 * works unchanged and the model recognises the id as a cell.
 */
@Composable
private fun TableCellField(model: EditorModel, cell: TableCell, header: Boolean = false) {
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
        header = header,
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
 * tall as its tallest cell; a table narrower than [viewport] shares the rest
 * in proportion to the columns' widths (CSS `table-layout: auto; width: 100%`). The hairline grid and the header tint are painted
 * behind the cells from the measured geometry, which is why an editable cell
 * (whose own height grows as it wraps) still gets a continuous border.
 */
@Composable
internal fun TableGridLayout(
    columnCount: Int,
    rowCount: Int,
    modifier: Modifier = Modifier,
    viewport: () -> Int = { 0 },
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
        val natural = widths.sum()
        val spare = viewport() - natural
        if (spare > 0 && natural > 0) {
            var given = 0
            for (col in 0 until columnCount) {
                val share = if (col == columnCount - 1) spare - given else spare * widths[col] / natural
                given += share
                widths[col] += share
            }
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
