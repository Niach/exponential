package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.ListType
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Editing-intent tests for EditorModel — the interactive paths the adversarial
 * review flagged (Enter over a selection, multi-line paste) which unit tests must
 * lock so they can't silently regress on-device.
 *
 * EditorModel is @Stable Compose state but its mutations are plain logic, so they
 * exercise fine off the main thread in a JVM test.
 */
class EditorModelTest {

    private fun model(markdown: String): EditorModel =
        EditorModel().apply { load(markdown) }

    private fun paras(m: EditorModel) = m.rows.filterIsInstance<EditorRow.Para>()

    @Test
    fun enterAtEndSplitsIntoTwoRows() {
        val m = model("Hello")
        // Field reports the post-edit text: caret at end, Enter → "Hello\n".
        m.splitParagraphFrom(paras(m).first().id, "Hello\n")
        val texts = paras(m).map { it.text }
        assertEquals(listOf("Hello", ""), texts)
    }

    @Test
    fun enterMidStringSplitsCorrectly() {
        val m = model("HelloWorld")
        // Caret after "Hello": Compose delivers "Hello\nWorld".
        m.splitParagraphFrom(paras(m).first().id, "Hello\nWorld")
        assertEquals(listOf("Hello", "World"), paras(m).map { it.text })
    }

    @Test
    fun enterOverSelectionReplacesNotDuplicates() {
        val m = model("HelloWorld")
        // Select "World" (5..10) and press Enter — Compose already removed the
        // selection, so the post-edit text is "Hello\n". The selected text must
        // NOT survive into the new line.
        m.splitParagraphFrom(paras(m).first().id, "Hello\n")
        assertEquals(listOf("Hello", ""), paras(m).map { it.text })
    }

    @Test
    fun multiLinePasteBecomesNRowsWithNothingDropped() {
        val m = model("")
        m.splitParagraphFrom(paras(m).first().id, "A\nB\nC")
        assertEquals(listOf("A", "B", "C"), paras(m).map { it.text })
        assertEquals("A\n\nB\n\nC", m.currentMarkdown())
    }

    @Test
    fun multiLinePasteAppendedToExistingText() {
        val m = model("Hello")
        // Caret at end, paste "A\nB" → "HelloA\nB".
        m.splitParagraphFrom(paras(m).first().id, "HelloA\nB")
        assertEquals(listOf("HelloA", "B"), paras(m).map { it.text })
    }

    @Test
    fun enterOnEmptyListItemExitsList() {
        val m = model("- item")
        val row = paras(m).first()
        assertEquals(ListType.Bullet, row.attrs.listType)
        // Clear the item then press Enter on the now-empty bullet.
        m.updatePara(row.id, "", 0)
        m.splitParagraphFrom(row.id, "\n")
        val after = paras(m).first()
        assertEquals(BlockKind.Paragraph, after.attrs.kind)
    }

    @Test
    fun enterContinuesBulletList() {
        val m = model("- one")
        val row = paras(m).first()
        assertEquals("one", row.text) // bullet glyph is rendered, not part of editable text
        // Caret at end of "one", press Enter → post-edit text "one\n".
        m.splitParagraphFrom(row.id, "one\n")
        val list = paras(m)
        assertEquals(2, list.size)
        assertTrue(list.all { it.attrs.listType == ListType.Bullet })
        assertEquals("", list.last().text)
    }

    @Test
    fun enterHandsFocusAndCaretToTheNewRow() {
        // EXP-25 regression: after a split the model must point BOTH the focus
        // target and the desired caret at the newly-created last row, and the
        // old row must not be able to consume the new row's caret seed.
        val m = model("HelloWorld")
        val oldRow = paras(m).first()
        m.splitParagraphFrom(oldRow.id, "Hello\nWorld")
        val newRow = paras(m).last()
        assertEquals(newRow.id, m.focusedRowId)
        assertEquals(newRow.id to 0, m.desiredSelection)
        // Row-scoped consumption: the old row gets nothing, the new row gets
        // caret 0 exactly once.
        assertNull(m.consumeDesiredSelection(oldRow.id))
        assertEquals(0, m.consumeDesiredSelection(newRow.id))
        assertNull(m.consumeDesiredSelection(newRow.id))
    }

    @Test
    fun toggleBoldOverSelectionMarksRange() {
        val m = model("hello world")
        val row = paras(m).first()
        m.toggleMark(row.id, 0..5, InlineKind.Bold)
        assertEquals("**hello** world", m.currentMarkdown())
    }

    // -- Ordered-list renumbering (REV-31: per-depth, never flat) --

    @Test
    fun enterOnNestedOrderedListKeepsPerDepthNumbering() {
        // Web-authored nested ordered list: renumbering after a structural edit
        // must keep a counter PER DEPTH — the flat walk used to rewrite b→2,
        // c→3, d→4, corrupting the stored bytes for every client.
        val m = model("1. a\n   1. b\n   2. c\n2. d")
        assertEquals(listOf(0, 1, 1, 0), paras(m).map { it.attrs.listDepth })
        assertEquals(listOf(1, 1, 2, 2), paras(m).map { it.attrs.orderedIndex })
        // Enter at the end of "d" appends a new depth-0 item.
        m.splitParagraphFrom(paras(m).last().id, "d\n")
        assertEquals(listOf(1, 1, 2, 2, 3), paras(m).map { it.attrs.orderedIndex })
        // Trailing "3. " loses its space to the serializer's final trim.
        assertEquals("1. a\n  1. b\n  2. c\n2. d\n3.", m.currentMarkdown())
    }

    @Test
    fun nestedBulletBetweenOrderedSiblingsDoesNotResetNumbering() {
        val m = model("1. a\n   - x\n2. d")
        m.splitParagraphFrom(paras(m).last().id, "d\n")
        // The nested bullet must not break the depth-0 ordered run: d stays 2.
        assertEquals(listOf(1, 0, 2, 3), paras(m).map { it.attrs.orderedIndex })
    }

    @Test
    fun sameDepthTypeSwitchStartsANewOrderedList() {
        // A bullet AT THE SAME depth ends the ordered list, so the ordered item
        // after it heads a new list and restarts at 1.
        val m = model("1. a\n- x\n1. b")
        m.splitParagraphFrom(paras(m).last().id, "b\n")
        assertEquals(listOf(1, 0, 1, 2), paras(m).map { it.attrs.orderedIndex })
    }

    @Test
    fun laterParentGetsAFreshNestedCounter() {
        // Each parent item's nested list is its own list — the second parent's
        // children restart at 1 instead of continuing the first parent's run.
        val m = model("1. a\n   1. b\n2. c\n   1. d\n   2. e")
        m.splitParagraphFrom(paras(m).last().id, "e\n")
        assertEquals(listOf(1, 1, 2, 1, 2, 3), paras(m).map { it.attrs.orderedIndex })
        assertEquals(listOf(0, 1, 0, 1, 1, 1), paras(m).map { it.attrs.listDepth })
    }

    // -- Image insertion (EXP-327: caret vs end-of-description) --

    @Test
    fun insertImageUrlSplitsAtTheCaret() {
        val m = model("before after")
        val row = paras(m).first()
        m.setFocused(row.id)
        m.updateSelection(row.id, 6..6)
        m.insertImageUrl("/api/attachments/x", alt = "image")
        assertEquals("before\n\n![image](/api/attachments/x)\n\nafter", m.currentMarkdown())
    }

    @Test
    fun appendImageUrlIgnoresTheCaretAndLandsLast() {
        // A picture picked through the FILE picker was an attach gesture, not a
        // typing one — it belongs after everything already written, whatever the
        // caret happens to be doing.
        val m = model("before after")
        val row = paras(m).first()
        m.setFocused(row.id)
        m.updateSelection(row.id, 6..6)
        m.appendImageUrl("/api/attachments/x", alt = "image")
        assertEquals("before after\n\n![image](/api/attachments/x)", m.currentMarkdown())
    }

    @Test
    fun typingDoesNotBumpRevision() {
        val m = model("ab")
        val row = paras(m).first()
        val before = m.revision(row.id)
        m.updatePara(row.id, "abc", 3)
        assertEquals(before, m.revision(row.id))
    }

    // -- Pending inline marks (collapsed-caret bold/italic, iOS typingAttributes) --

    @Test
    fun pendingBoldAppliesToNextTypedChar() {
        val m = model("ab")
        val row = paras(m).first()
        // Tap Bold with a collapsed caret at end → queued, not yet visible.
        m.togglePendingMark(row.id, 2, InlineKind.Bold)
        assertTrue(m.pendingMarkActive(row.id, 2, InlineKind.Bold))
        // Type 'c' at the caret → it inherits the queued mark.
        m.updatePara(row.id, "abc", 3)
        assertEquals("ab**c**", m.currentMarkdown())
    }

    @Test
    fun pendingBoldKeepsInheritingConsecutiveChars() {
        val m = model("ab")
        val row = paras(m).first()
        m.togglePendingMark(row.id, 2, InlineKind.Bold)
        m.updatePara(row.id, "abc", 3)
        m.updatePara(row.id, "abcd", 4)
        assertEquals("ab**cd**", m.currentMarkdown())
    }

    @Test
    fun movingCaretClearsPendingMark() {
        val m = model("ab")
        val row = paras(m).first()
        m.togglePendingMark(row.id, 2, InlineKind.Bold)
        // Caret moves (no text change) → the queue drops.
        m.updateSelection(row.id, 0..0)
        assertTrue(!m.pendingMarkActive(row.id, 2, InlineKind.Bold))
        m.updatePara(row.id, "xab", 1)
        assertEquals("xab", m.currentMarkdown())
    }

    @Test
    fun togglingPendingMarkTwiceCancels() {
        val m = model("ab")
        val row = paras(m).first()
        m.togglePendingMark(row.id, 2, InlineKind.Bold)
        m.togglePendingMark(row.id, 2, InlineKind.Bold)
        assertTrue(!m.pendingMarkActive(row.id, 2, InlineKind.Bold))
        m.updatePara(row.id, "abc", 3)
        assertEquals("abc", m.currentMarkdown())
    }
}
