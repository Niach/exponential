package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ContentBlock
import com.exponential.app.ui.markdown.model.ListType
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The flatten/unflatten between blocks and editor rows must be lossless: a
 * markdown string parsed to blocks, projected to rows, and folded back to blocks
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

    @Test fun everyTextLineBecomesOneParaRow() {
        val rows = EditorRows.fromBlocks(MarkdownParser.parse("# T\n\nbody"))
        val paras = rows.filterIsInstance<EditorRow.Para>()
        // heading line + body line (the empty separator collapses out).
        assertEquals(BlockKind.Heading, paras.first().attrs.kind)
        assertTrue(paras.any { it.text == "body" })
    }

    @Test fun imageRowSplitsParaRuns() {
        val rows = EditorRows.fromBlocks(MarkdownParser.parse("a\n\n![x](/api/attachments/y)\n\nb"))
        val kinds = rows.map { it::class.simpleName }
        assertEquals(listOf("Para", "Image", "Para"), kinds)
    }

    @Test fun orderedListIndicesPreserved() {
        val rows = EditorRows.fromBlocks(MarkdownParser.parse("1. one\n2. two\n3. three"))
            .filterIsInstance<EditorRow.Para>()
        assertEquals(listOf(1, 2, 3), rows.map { it.attrs.orderedIndex })
        assertTrue(rows.all { it.attrs.listType == ListType.Ordered })
    }
}
