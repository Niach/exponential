package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ContentBlock
import com.exponential.app.ui.markdown.model.ListType
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The flatten/unflatten between blocks and editor rows must be lossless: a
 * markdown string parsed to blocks, boarded to rows, and folded back to blocks
 * must serialize to the same markdown.
 */
class EditorRowsTest {

    private fun roundTripViaRows(md: String): String {
        val blocks = MarkdownParser.parse(md)
        val rows = EditorRows.fromBlocks(blocks)
        val back = EditorRows.toBlocks(rows)
        return MarkdownSerializer.blocksToMarkdown(back)
    }

    @Test fun paragraphsSurviveRowRoundTrip() =
        assertEquals("First.\n\nSecond.", roundTripViaRows("First.\n\nSecond."))

    @Test fun listSurvivesRowRoundTrip() =
        assertEquals("- a\n- b\n- c", roundTripViaRows("- a\n- b\n- c"))

    @Test fun headingAndBodySurvive() =
        assertEquals("# T\n\nbody", roundTripViaRows("# T\n\nbody"))

    @Test fun imageSurvivesRowRoundTrip() =
        assertEquals(
            "before\n\n![a](/api/attachments/x)\n\nafter",
            roundTripViaRows("before\n\n![a](/api/attachments/x)\n\nafter"),
        )

    @Test fun marksSurviveRowRoundTrip() =
        assertEquals("a **b** c", roundTripViaRows("a **b** c"))

    @Test fun codeBlockSurvivesRowRoundTrip() =
        assertEquals("```js\nx\ny\n```", roundTripViaRows("```js\nx\ny\n```"))

    @Test fun everyTextBlockBecomesOneMultiLineRun() {
        val rows = EditorRows.fromBlocks(MarkdownParser.parse("# T\n\nbody"))
        val run = rows.filterIsInstance<EditorRow.TextRun>().single()
        // heading line + body line in ONE field (EXP-534 — selection spans them).
        assertEquals(listOf("T", "body"), run.lines)
        assertEquals(BlockKind.Heading, run.paragraphs.first().kind)
        assertEquals(run.lines.size, run.paragraphs.size)
    }

    @Test fun imageRowSplitsTextRuns() {
        val rows = EditorRows.fromBlocks(MarkdownParser.parse("a\n\n![x](/api/attachments/y)\n\nb"))
        val kinds = rows.map { it::class.simpleName }
        assertEquals(listOf("TextRun", "Image", "TextRun"), kinds)
    }

    @Test fun orderedListIndicesPreserved() {
        val run = EditorRows.fromBlocks(MarkdownParser.parse("1. one\n2. two\n3. three"))
            .filterIsInstance<EditorRow.TextRun>().single()
        assertEquals(listOf(1, 2, 3), run.paragraphs.map { it.orderedIndex })
        assertTrue(run.paragraphs.all { it.listType == ListType.Ordered })
    }

    // --- Tables (EXP-726) are rows in their own right, exactly like images. ---

    @Test fun tableSurvivesRowRoundTrip() =
        assertEquals(
            "| a | b |\n| --- | --- |\n| 1 | 2 |",
            roundTripViaRows("| a | b |\n| --- | --- |\n| 1 | 2 |"),
        )

    @Test fun tableBetweenParagraphsSurvivesRowRoundTrip() =
        assertEquals(
            "before\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nafter",
            roundTripViaRows("before\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nafter"),
        )

    @Test fun tableRowIsPaddedWithTextRuns() {
        val rows = EditorRows.fromBlocks(MarkdownParser.parse("| a |\n| --- |\n| 1 |"))
        assertEquals(listOf("TextRun", "Table", "TextRun"), rows.map { it::class.simpleName })
    }

    @Test fun cellIdsAreStableAcrossTheRowRoundTrip() {
        val rows = EditorRows.fromBlocks(MarkdownParser.parse("| a | b |\n| --- | --- |\n| 1 | 2 |"))
        val table = rows.filterIsInstance<EditorRow.Table>().single().table
        val ids = table.allCells.map { it.id }
        assertEquals(4, ids.toSet().size)
        val back = EditorRows.toBlocks(rows).filterIsInstance<ContentBlock.TableBlock>().single()
        assertEquals(ids, back.table.allCells.map { it.id })
        assertEquals(0 to 1, table.locate(table.header[1].id))
        assertEquals(1 to 0, table.locate(table.rows[0][0].id))
    }
}
