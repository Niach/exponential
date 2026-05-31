package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ContentBlock
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.ListType
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
}
