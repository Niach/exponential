package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ContentBlock
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.ListType
import com.exponential.app.ui.markdown.model.TableAlignment
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Structural assertions on the parsed block model (beyond round-trip parity). */
class MarkdownParserTest {

    private fun textBlock(md: String): ContentBlock.TextBlock =
        MarkdownParser.parse(md).filterIsInstance<ContentBlock.TextBlock>().first { it.content.text.isNotEmpty() }

    @Test
    fun taskListUncheckedStaysChecklist() {
        val tb = textBlock("- [ ] open")
        val p = tb.content.paragraphs.first()
        assertEquals(BlockKind.ListItem, p.kind)
        assertEquals(ListType.Checklist, p.listType)
        assertEquals(false, p.checked)
        assertEquals("open", tb.content.text)
    }

    @Test
    fun taskListCheckedDetected() {
        val tb = textBlock("- [x] done")
        val p = tb.content.paragraphs.first()
        assertEquals(ListType.Checklist, p.listType)
        assertEquals(true, p.checked)
        assertEquals("done", tb.content.text)
    }

    @Test
    fun plainBulletIsNotChecklist() {
        val tb = textBlock("- item")
        assertEquals(ListType.Bullet, tb.content.paragraphs.first().listType)
    }

    @Test
    fun headingLevelParsed() {
        val tb = textBlock("### Three")
        val p = tb.content.paragraphs.first()
        assertEquals(BlockKind.Heading, p.kind)
        assertEquals(3, p.headingLevel)
    }

    @Test
    fun imageSplitsIntoOwnBlock() {
        val blocks = MarkdownParser.parse("text\n\n![a](/api/attachments/x)\n\nmore")
        val images = blocks.filterIsInstance<ContentBlock.ImageBlock>()
        assertEquals(1, images.size)
        assertEquals("/api/attachments/x", images.first().url)
        assertEquals("a", images.first().alt)
    }

    @Test
    fun blockDocumentAlwaysStartsAndEndsWithText() {
        val blocks = MarkdownParser.parse("![only](/api/attachments/x)")
        assertTrue(blocks.first() is ContentBlock.TextBlock)
        assertTrue(blocks.last() is ContentBlock.TextBlock)
        assertEquals(1, blocks.filterIsInstance<ContentBlock.ImageBlock>().size)
    }

    @Test
    fun boldMarkRangeIsExact() {
        val tb = textBlock("a **bc** d")
        assertEquals("a bc d", tb.content.text)
        val bold = tb.content.marks.first { it.kind == InlineKind.Bold }
        assertEquals("bc", tb.content.text.substring(bold.start, bold.end))
    }

    @Test
    fun linkPreservesRelativeHref() {
        val tb = textBlock("[t](/rel/path)")
        val link = tb.content.marks.first { it.kind == InlineKind.Link }
        assertEquals("/rel/path", link.href)
    }

    @Test
    fun emptyInputIsSingleEmptyTextBlock() {
        val blocks = MarkdownParser.parse("")
        assertEquals(1, blocks.size)
        assertTrue(blocks.first() is ContentBlock.TextBlock)
        assertEquals("", (blocks.first() as ContentBlock.TextBlock).content.text)
    }

    // EXP-440: agent narration is terminal output, so read-only renderers can
    // ask for its soft breaks to stay real lines. Editable surfaces keep the
    // GFM space — anything else would rewrite the stored bytes on save.

    @Test
    fun softBreakIsASpaceByDefault() {
        val tb = textBlock("one\ntwo")
        assertEquals("one two", tb.content.text)
        assertEquals(1, tb.content.paragraphs.size)
    }

    @Test
    fun softBreakBecomesItsOwnLineWhenRequested() {
        val blocks = MarkdownParser.parse("one\ntwo", softBreaksAsNewlines = true)
        val tb = blocks.filterIsInstance<ContentBlock.TextBlock>()
            .first { it.content.text.isNotEmpty() }
        assertEquals("one\ntwo", tb.content.text)
        // The RichText invariant: one ParagraphAttrs per '\n'-delimited line.
        assertEquals(tb.content.lines.size, tb.content.paragraphs.size)
    }

    @Test
    fun softBreakLinesKeepTheirBlockAttributes() {
        val blocks = MarkdownParser.parse("> quoted\ncontinued", softBreaksAsNewlines = true)
        val tb = blocks.filterIsInstance<ContentBlock.TextBlock>()
            .first { it.content.text.isNotEmpty() }
        assertEquals("quoted\ncontinued", tb.content.text)
        assertTrue(tb.content.paragraphs.all { it.kind == BlockKind.Blockquote })
    }

    // --- GFM tables (EXP-726) ---

    private fun table(md: String) =
        MarkdownParser.parse(md).filterIsInstance<ContentBlock.TableBlock>().single().table

    @Test
    fun pipeTableBecomesATableBlock() {
        val t = table("| a | b |\n| --- | --- |\n| 1 | 2 |")
        assertEquals(2, t.columnCount)
        assertEquals(listOf("a", "b"), t.header.map { it.text })
        assertEquals(listOf(listOf("1", "2")), t.rows.map { row -> row.map { it.text } })
    }

    @Test
    fun headerCellsCarryTheColumnAlignments() {
        val t = table("| l | c | r | n |\n| :--- | :---: | ---: | --- |\n| 1 | 2 | 3 | 4 |")
        assertEquals(
            listOf(TableAlignment.Left, TableAlignment.Center, TableAlignment.Right, TableAlignment.None),
            t.alignments,
        )
    }

    @Test
    fun headerOnlyTableHasNoBodyRows() {
        val t = table("| a | b |\n| --- | --- |")
        assertEquals(listOf("a", "b"), t.header.map { it.text })
        assertTrue(t.rows.isEmpty())
    }

    @Test
    fun raggedBodyRowIsPaddedToTheHeaderWidth() {
        val t = table("| a | b | c |\n| --- | --- | --- |\n| 1 |")
        assertEquals(listOf(listOf("1", "", "")), t.rows.map { row -> row.map { it.text } })
    }

    @Test
    fun tableBetweenParagraphsKeepsItsTextNeighbours() {
        val kinds = MarkdownParser.parse("before\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nafter")
            .filter { it !is ContentBlock.TextBlock || it.content.text.isNotEmpty() }
            .map { it::class.simpleName }
        assertEquals(listOf("TextBlock", "TableBlock", "TextBlock"), kinds)
    }

    @Test
    fun cellMarksCarryCellLocalOffsets() {
        val t = table("| a **b** | c |\n| --- | --- |\n| 1 | 2 |")
        assertEquals("a b", t.header[0].text)
        val mark = t.header[0].marks.single()
        assertEquals(InlineKind.Bold, mark.kind)
        assertEquals(2, mark.start)
        assertEquals(3, mark.end)
    }

    @Test
    fun imageInsideACellStaysLiteralText() {
        val t = table("| ![a](/api/attachments/x) | c |\n| --- | --- |\n| 1 | 2 |")
        assertEquals("![a](/api/attachments/x)", t.header[0].text)
        assertTrue(MarkdownParser.parse("| ![a](/x) |\n| --- |").none { it is ContentBlock.ImageBlock })
    }

    /**
     * commonmark-java splits a row on unescaped pipes and unescapes `\|`
     * BEFORE the inline parser runs, so a code span's `\|` reaches us as a
     * bare `|` — which is exactly why the serializer may escape pipes over the
     * whole rendered cell, code spans included, and still round-trip.
     */
    @Test
    fun escapedPipeInsideACodeSpanIsUnescaped() {
        val t = table("| `a \\| b` | c |\n| --- | --- |\n| 1 | 2 |")
        assertEquals("a | b", t.header[0].text)
        assertEquals(InlineKind.InlineCode, t.header[0].marks.single().kind)
    }
}
