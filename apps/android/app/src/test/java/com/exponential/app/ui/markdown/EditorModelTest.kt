package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.ListType
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Editing-intent tests for EditorModel. Since EXP-534 the document is one
 * multi-line [EditorRow.TextRun] per stretch of text between images, and every
 * IME edit — typing, Enter (a plain '\n'), backspace over a newline,
 * multi-line paste — flows through [EditorModel.updateRun]. These lock the
 * paragraph-remap semantics the old one-field-per-line editor guaranteed
 * structurally.
 *
 * EditorModel is @Stable Compose state but its mutations are plain logic, so they
 * exercise fine off the main thread in a JVM test.
 */
class EditorModelTest {

    private fun model(markdown: String): EditorModel =
        EditorModel().apply { load(markdown) }

    private fun runs(m: EditorModel) = m.rows.filterIsInstance<EditorRow.TextRun>()
    private fun run(m: EditorModel) = runs(m).single()

    @Test
    fun enterAtEndAddsALine() {
        val m = model("Hello")
        // Field reports the post-edit text: caret at end, Enter → "Hello\n".
        m.updateRun(run(m).id, "Hello\n", 6)
        assertEquals(listOf("Hello", ""), run(m).lines)
        assertEquals(2, run(m).paragraphs.size)
    }

    @Test
    fun enterMidStringSplitsTheLine() {
        val m = model("HelloWorld")
        // Caret after "Hello": Compose delivers "Hello\nWorld".
        m.updateRun(run(m).id, "Hello\nWorld", 6)
        assertEquals(listOf("Hello", "World"), run(m).lines)
    }

    @Test
    fun enterOverSelectionReplacesNotDuplicates() {
        val m = model("HelloWorld")
        // Select "World" (5..10) and press Enter — Compose already removed the
        // selection, so the post-edit text is "Hello\n". The selected text must
        // NOT survive into the new line.
        m.updateRun(run(m).id, "Hello\n", 6)
        assertEquals(listOf("Hello", ""), run(m).lines)
    }

    @Test
    fun multiLinePasteBecomesNLinesWithNothingDropped() {
        val m = model("")
        m.updateRun(run(m).id, "A\nB\nC", 5)
        assertEquals(listOf("A", "B", "C"), run(m).lines)
        assertEquals("A\n\nB\n\nC", m.currentMarkdown())
    }

    @Test
    fun multiLinePasteAppendedToExistingText() {
        val m = model("Hello")
        // Caret at end, paste "A\nB" → "HelloA\nB".
        m.updateRun(run(m).id, "HelloA\nB", 8)
        assertEquals(listOf("HelloA", "B"), run(m).lines)
    }

    @Test
    fun enterOnEmptyListItemExitsList() {
        val m = model("- item")
        assertEquals(ListType.Bullet, run(m).paragraphs.first().listType)
        // Clear the item then press Enter on the now-empty bullet: the '\n' is
        // dropped and the line demotes to a plain paragraph.
        m.updateRun(run(m).id, "", 0)
        m.updateRun(run(m).id, "\n", 1)
        assertEquals("", run(m).text)
        assertEquals(BlockKind.Paragraph, run(m).paragraphs.first().kind)
        // The field holds the '\n' the model dropped — it must be reseeded.
        assertEquals(run(m).id to 0, m.desiredSelection)
    }

    @Test
    fun enterOnEmptyListItemBetweenLinesExitsThatItemOnly() {
        // The caret-pinned diff must attribute the '\n' to the EMPTY middle
        // item, not to the greedy prefix-match position inside "b"'s line.
        val m = model("- a\n- x\n- b")
        m.updateRun(run(m).id, "a\n\nb", 2) // clear "x"
        assertEquals(listOf("a", "", "b"), run(m).lines)
        m.updateRun(run(m).id, "a\n\n\nb", 3) // Enter on the empty middle item
        assertEquals("a\n\nb", run(m).text)
        assertEquals(
            listOf(BlockKind.ListItem, BlockKind.Paragraph, BlockKind.ListItem),
            run(m).paragraphs.map { it.kind },
        )
    }

    @Test
    fun enterContinuesBulletList() {
        val m = model("- one")
        assertEquals("one", run(m).text) // bullet glyph is rendered, not part of editable text
        // Caret at end of "one", press Enter → post-edit text "one\n".
        m.updateRun(run(m).id, "one\n", 4)
        assertEquals(2, run(m).paragraphs.size)
        assertTrue(run(m).paragraphs.all { it.listType == ListType.Bullet })
        assertEquals("", run(m).lines.last())
    }

    @Test
    fun enterChecklistContinuesUnchecked() {
        val m = model("- [x] done")
        assertTrue(run(m).paragraphs.first().checked)
        m.updateRun(run(m).id, "done\n", 5)
        assertEquals(listOf(true, false), run(m).paragraphs.map { it.checked })
        assertTrue(run(m).paragraphs.all { it.listType == ListType.Checklist })
    }

    @Test
    fun enterDoesNotReseedTheField() {
        // The field already holds the post-Enter text with the caret in place —
        // a revision bump would clobber fast typing (EXP-25 discipline). Only
        // rewrites (list exit) may reseed.
        val m = model("Hello")
        val before = m.revision(run(m).id)
        m.updateRun(run(m).id, "Hello\nWorld", 6)
        assertEquals(before, m.revision(run(m).id))
        assertNull(m.desiredSelection)
    }

    @Test
    fun backspaceOverNewlineMergesKeepingFirstLineAttrs() {
        val m = model("# T\n\nbody")
        assertEquals(listOf("T", "body"), run(m).lines)
        // Backspace at the start of "body" deletes the '\n' — the merged line
        // keeps the heading's attrs (first-line wins, per-row-merge parity).
        m.updateRun(run(m).id, "Tbody", 1)
        assertEquals(listOf("Tbody"), run(m).lines)
        assertEquals(listOf(BlockKind.Heading), run(m).paragraphs.map { it.kind })
    }

    @Test
    fun toggleBoldOverSelectionMarksRange() {
        val m = model("hello world")
        val row = run(m)
        m.toggleMark(row.id, 0..5, InlineKind.Bold)
        assertEquals("**hello** world", m.currentMarkdown())
    }

    // -- Rail ops: headings + clear formatting (EXP-568) ---------------------

    @Test
    fun setHeadingAppliesTheLevelToTheCaretParagraph() {
        val m = model("hello")
        val row = run(m)
        m.setFocused(row.id)
        m.updateSelection(row.id, 0..0)
        m.setHeading(row.id, 2)
        assertEquals(listOf(BlockKind.Heading), run(m).paragraphs.map { it.kind })
        assertEquals(2, run(m).paragraphs.first().headingLevel)
        assertEquals("## hello", m.currentMarkdown())
    }

    @Test
    fun setHeadingToTheActiveLevelClearsIt() {
        val m = model("## hello")
        val row = run(m)
        m.setFocused(row.id)
        m.updateSelection(row.id, 0..0)
        // Tapping the already-active level is its own off switch.
        m.setHeading(row.id, 2)
        assertEquals(listOf(BlockKind.Paragraph), run(m).paragraphs.map { it.kind })
        assertEquals("hello", m.currentMarkdown())
    }

    @Test
    fun setHeadingZeroReturnsToAPlainParagraph() {
        val m = model("### hello")
        val row = run(m)
        m.setFocused(row.id)
        m.updateSelection(row.id, 0..0)
        m.setHeading(row.id, 0)
        assertEquals(listOf(BlockKind.Paragraph), run(m).paragraphs.map { it.kind })
        assertEquals("hello", m.currentMarkdown())
    }

    @Test
    fun setHeadingCoversEveryParagraphTheSelectionTouches() {
        val m = model("a\n\nb\n\nc")
        val row = run(m)
        m.setFocused(row.id)
        m.updateSelection(row.id, 0..3) // "a\nb"
        m.setHeading(row.id, 1)
        assertEquals(
            listOf(BlockKind.Heading, BlockKind.Heading, BlockKind.Paragraph),
            run(m).paragraphs.map { it.kind },
        )
        assertEquals("# a\n\n# b\n\nc", m.currentMarkdown())
    }

    @Test
    fun clearFormattingStripsEveryInlineMarkOverTheSelection() {
        val m = model("hello world")
        val row = run(m)
        m.toggleMark(row.id, 0..5, InlineKind.Bold)
        m.toggleMark(row.id, 0..5, InlineKind.Italic)
        m.toggleMark(row.id, 6..11, InlineKind.Link, "https://example.com")
        assertTrue(m.currentMarkdown() != "hello world")
        m.clearFormatting(row.id, 0..11)
        assertEquals("hello world", m.currentMarkdown())
        assertTrue(run(m).marks.isEmpty())
    }

    @Test
    fun clearFormattingLeavesMarksOutsideTheSelectionAlone() {
        val m = model("hello world")
        val row = run(m)
        m.toggleMark(row.id, 0..5, InlineKind.Bold)
        m.toggleMark(row.id, 6..11, InlineKind.Bold)
        m.clearFormatting(row.id, 6..11)
        assertEquals("**hello** world", m.currentMarkdown())
    }

    @Test
    fun clearFormattingAtACollapsedCaretResetsTheParagraph() {
        val m = model("> quoted")
        val row = run(m)
        assertEquals(listOf(BlockKind.Blockquote), row.paragraphs.map { it.kind })
        m.setFocused(row.id)
        m.updateSelection(row.id, 0..0)
        // Nothing inline to strip, so the block formatting is what clears
        // (web parity: unsetAllMarks + clearNodes).
        m.clearFormatting(row.id, 0..0)
        assertEquals(listOf(BlockKind.Paragraph), run(m).paragraphs.map { it.kind })
        assertEquals("quoted", m.currentMarkdown())
    }

    @Test
    fun clearFormattingDropsQueuedPendingMarks() {
        val m = model("hello")
        val row = run(m)
        m.setFocused(row.id)
        m.updateSelection(row.id, 5..5)
        m.togglePendingMark(row.id, 5, InlineKind.Bold)
        assertTrue(m.pendingMarkActive(row.id, 5, InlineKind.Bold))
        m.clearFormatting(row.id, 5..5)
        assertFalse(m.pendingMarkActive(row.id, 5, InlineKind.Bold))
    }

    // -- Paragraph-kind toggles over a selection (multi-paragraph, EXP-534) --

    @Test
    fun toggleListOverMultiLineSelectionListsEveryTouchedParagraph() {
        val m = model("a\n\nb\n\nc")
        val row = run(m)
        assertEquals("a\nb\nc", row.text)
        m.setFocused(row.id)
        m.updateSelection(row.id, 0..3) // "a\nb"
        m.toggleList(row.id, ListType.Bullet)
        assertEquals(
            listOf(BlockKind.ListItem, BlockKind.ListItem, BlockKind.Paragraph),
            run(m).paragraphs.map { it.kind },
        )
        assertEquals("- a\n- b\n\nc", m.currentMarkdown())
    }

    @Test
    fun toggleListIsAllOrNothingOverTheSelection() {
        val m = model("- a\n- b")
        val row = run(m)
        m.setFocused(row.id)
        m.updateSelection(row.id, 0..3)
        // Every touched paragraph already has the type → all clear to plain.
        m.toggleList(row.id, ListType.Bullet)
        assertTrue(run(m).paragraphs.all { it.kind == BlockKind.Paragraph })
    }

    @Test
    fun collapsedCaretTogglesOnlyItsParagraph() {
        val m = model("a\n\nb")
        val row = run(m)
        m.setFocused(row.id)
        m.updateSelection(row.id, 2..2) // caret on "b"
        m.toggleQuote(row.id)
        assertEquals(
            listOf(BlockKind.Paragraph, BlockKind.Blockquote),
            run(m).paragraphs.map { it.kind },
        )
    }

    // -- Backspace intents ---------------------------------------------------

    @Test
    fun clearParagraphFormatDemotesTheCaretLineOnly() {
        val m = model("- a\n- b")
        val row = run(m)
        // Backspace at the start of "b" (caret 2 = its line start).
        m.clearParagraphFormat(row.id, 2)
        assertEquals(
            listOf(BlockKind.ListItem, BlockKind.Paragraph),
            run(m).paragraphs.map { it.kind },
        )
        assertEquals(row.text, run(m).text)
    }

    @Test
    fun backspaceAtRunStartDeletesTheImageAndMergesRuns() {
        val m = model("a\n\n![x](/api/attachments/y)\n\nb")
        val second = runs(m).last()
        m.backspaceAtRunStart(second.id)
        assertEquals("ab", m.currentMarkdown())
        val merged = run(m)
        assertEquals(merged.id, m.focusedRowId)
        assertEquals(merged.id to 1, m.desiredSelection)
    }

    @Test
    fun deleteImageRowMergesMultiLineNeighbors() {
        val m = model("a\n\n![x](/api/attachments/y)\n\n# H\n\nc")
        val image = m.rows.filterIsInstance<EditorRow.Image>().single()
        m.deleteImageRow(image.id)
        val merged = run(m)
        // "a" joins H's line (first-line attrs win: plain), "c" keeps its own.
        assertEquals(listOf("aH", "c"), merged.lines)
        assertEquals(
            listOf(BlockKind.Paragraph, BlockKind.Paragraph),
            merged.paragraphs.map { it.kind },
        )
    }

    // -- Ordered-list renumbering (REV-31: per-depth, never flat) --

    @Test
    fun enterOnNestedOrderedListKeepsPerDepthNumbering() {
        // Web-authored nested ordered list: renumbering after a structural edit
        // must keep a counter PER DEPTH — the flat walk used to rewrite b→2,
        // c→3, d→4, corrupting the stored bytes for every client.
        val m = model("1. a\n   1. b\n   2. c\n2. d")
        assertEquals(listOf(0, 1, 1, 0), run(m).paragraphs.map { it.listDepth })
        assertEquals(listOf(1, 1, 2, 2), run(m).paragraphs.map { it.orderedIndex })
        // Enter at the end of "d" appends a new depth-0 item.
        m.updateRun(run(m).id, run(m).text + "\n", run(m).text.length + 1)
        assertEquals(listOf(1, 1, 2, 2, 3), run(m).paragraphs.map { it.orderedIndex })
        // Trailing "3. " loses its space to the serializer's final trim.
        assertEquals("1. a\n  1. b\n  2. c\n2. d\n3.", m.currentMarkdown())
    }

    @Test
    fun nestedBulletBetweenOrderedSiblingsDoesNotResetNumbering() {
        val m = model("1. a\n   - x\n2. d")
        m.updateRun(run(m).id, run(m).text + "\n", run(m).text.length + 1)
        // The nested bullet must not break the depth-0 ordered run: d stays 2.
        assertEquals(listOf(1, 0, 2, 3), run(m).paragraphs.map { it.orderedIndex })
    }

    @Test
    fun sameDepthTypeSwitchStartsANewOrderedList() {
        // A bullet AT THE SAME depth ends the ordered list, so the ordered item
        // after it heads a new list and restarts at 1.
        val m = model("1. a\n- x\n1. b")
        m.updateRun(run(m).id, run(m).text + "\n", run(m).text.length + 1)
        assertEquals(listOf(1, 0, 1, 2), run(m).paragraphs.map { it.orderedIndex })
    }

    @Test
    fun laterParentGetsAFreshNestedCounter() {
        // Each parent item's nested list is its own list — the second parent's
        // children restart at 1 instead of continuing the first parent's run.
        val m = model("1. a\n   1. b\n2. c\n   1. d\n   2. e")
        m.updateRun(run(m).id, run(m).text + "\n", run(m).text.length + 1)
        assertEquals(listOf(1, 1, 2, 1, 2, 3), run(m).paragraphs.map { it.orderedIndex })
        assertEquals(listOf(0, 1, 0, 1, 1, 1), run(m).paragraphs.map { it.listDepth })
    }

    // -- Image insertion (EXP-327: caret vs end-of-description) --

    @Test
    fun insertImageUrlSplitsAtTheCaret() {
        val m = model("before after")
        val row = run(m)
        m.setFocused(row.id)
        m.updateSelection(row.id, 6..6)
        m.insertImageUrl("/api/attachments/x", alt = "image")
        assertEquals("before\n\n![image](/api/attachments/x)\n\nafter", m.currentMarkdown())
    }

    @Test
    fun insertImageMidRunKeepsLaterParagraphAttrs() {
        val m = model("a\n\n- item\n\n- two")
        val row = run(m)
        assertEquals(listOf("a", "item", "two"), row.lines)
        m.setFocused(row.id)
        m.updateSelection(row.id, 2..2) // start of "item"
        m.insertImageUrl("/api/attachments/x", alt = "image")
        val (first, second) = runs(m)
        // The prefix keeps its (now empty) caret line with the item's attrs —
        // the same empty-list-item stub the per-row editor produced.
        assertEquals(listOf("a", ""), first.lines)
        // The remainder's FIRST line demotes to plain (cut loose from its
        // block), later lines keep their attrs.
        assertEquals(listOf("item", "two"), second.lines)
        assertEquals(
            listOf(BlockKind.Paragraph, BlockKind.ListItem),
            second.paragraphs.map { it.kind },
        )
    }

    @Test
    fun appendImageUrlIgnoresTheCaretAndLandsLast() {
        // A picture picked through the FILE picker was an attach gesture, not a
        // typing one — it belongs after everything already written, whatever the
        // caret happens to be doing.
        val m = model("before after")
        val row = run(m)
        m.setFocused(row.id)
        m.updateSelection(row.id, 6..6)
        m.appendImageUrl("/api/attachments/x", alt = "image")
        assertEquals("before after\n\n![image](/api/attachments/x)", m.currentMarkdown())
    }

    @Test
    fun typingDoesNotBumpRevision() {
        val m = model("ab")
        val row = run(m)
        val before = m.revision(row.id)
        m.updateRun(row.id, "abc", 3)
        assertEquals(before, m.revision(row.id))
    }

    // -- Pending inline marks (collapsed-caret bold/italic, iOS typingAttributes) --

    @Test
    fun pendingBoldAppliesToNextTypedChar() {
        val m = model("ab")
        val row = run(m)
        // Tap Bold with a collapsed caret at end → queued, not yet visible.
        m.togglePendingMark(row.id, 2, InlineKind.Bold)
        assertTrue(m.pendingMarkActive(row.id, 2, InlineKind.Bold))
        // Type 'c' at the caret → it inherits the queued mark.
        m.updateRun(row.id, "abc", 3)
        assertEquals("ab**c**", m.currentMarkdown())
    }

    @Test
    fun pendingBoldKeepsInheritingConsecutiveChars() {
        val m = model("ab")
        val row = run(m)
        m.togglePendingMark(row.id, 2, InlineKind.Bold)
        m.updateRun(row.id, "abc", 3)
        m.updateRun(row.id, "abcd", 4)
        assertEquals("ab**cd**", m.currentMarkdown())
    }

    @Test
    fun movingCaretClearsPendingMark() {
        val m = model("ab")
        val row = run(m)
        m.togglePendingMark(row.id, 2, InlineKind.Bold)
        // Caret moves (no text change) → the queue drops.
        m.updateSelection(row.id, 0..0)
        assertTrue(!m.pendingMarkActive(row.id, 2, InlineKind.Bold))
        m.updateRun(row.id, "xab", 1)
        assertEquals("xab", m.currentMarkdown())
    }

    @Test
    fun togglingPendingMarkTwiceCancels() {
        val m = model("ab")
        val row = run(m)
        m.togglePendingMark(row.id, 2, InlineKind.Bold)
        m.togglePendingMark(row.id, 2, InlineKind.Bold)
        assertTrue(!m.pendingMarkActive(row.id, 2, InlineKind.Bold))
        m.updateRun(row.id, "abc", 3)
        assertEquals("abc", m.currentMarkdown())
    }

    // --- Emoji inserts (EXP-551) --------------------------------------------
    //
    // insertPlainText is the ONE path every emoji affordance uses (toolbar,
    // comment composer, comment editor). Emoji are multi-UTF-16-unit, so these
    // lock that the caret lands AFTER the whole sequence and that marks around
    // the insertion point shift by its full UTF-16 length.

    @Test
    fun insertingASurrogatePairEmojiLandsTheCaretAfterIt() {
        val m = model("ab")
        val row = run(m)
        m.setFocused(row.id)
        m.updateSelection(row.id, 1..1)
        m.insertPlainText("\uD83C\uDF89")
        assertEquals("a\uD83C\uDF89b", run(m).text)
        // 1 + the emoji's two UTF-16 units.
        assertEquals(row.id to 3, m.desiredSelection)
    }

    @Test
    fun insertingAZwjSequenceKeepsItIntact() {
        val m = model("")
        val row = run(m)
        m.setFocused(row.id)
        val family = "\uD83D\uDC69\u200D\uD83D\uDCBB"
        m.insertPlainText(family)
        assertEquals(family, run(m).text)
        assertEquals(family, m.currentMarkdown())
        assertEquals(row.id to family.length, m.desiredSelection)
    }

    @Test
    fun insertingASkinTonedEmojiShiftsMarksByItsFullLength() {
        val m = model("**ab**")
        val row = run(m)
        assertEquals("ab", row.text)
        m.setFocused(row.id)
        m.updateSelection(row.id, 0..0)
        val toned = "\uD83D\uDC4D\uD83C\uDFFD"
        m.insertPlainText(toned)
        // The bold run still covers exactly "ab", now pushed right by 4 units.
        assertEquals(toned + "**ab**", m.currentMarkdown())
    }

    @Test
    fun anEmojiInsertNeverArmsTheAutocomplete() {
        val m = model("hi")
        val row = run(m)
        m.setFocused(row.id)
        m.insertPlainText("\uD83C\uDF89")
        assertTrue(!m.consumeAutocompleteArm(row.id))
        // The `@`/`#` affordances still do.
        m.insertPlainText("@")
        assertTrue(m.consumeAutocompleteArm(row.id))
    }

    // -- Links (EXP-572) --------------------------------------------------------

    @Test
    fun linkRangeAtCoversTheWholeLinkUnderACaret() {
        val m = model("see [docs](https://a.example) now")
        val id = run(m).id
        // Caret inside "docs" (text is "see docs now", link = 4..8).
        assertEquals(4..8, m.linkRangeAt(id, 6..6))
        assertEquals("https://a.example", m.linkHrefAt(id, 6..6))
        assertNull(m.linkRangeAt(id, 1..1))
        // The end offset is exclusive: a caret right after the link is outside it.
        assertNull(m.linkRangeAt(id, 8..8))
    }

    @Test
    fun relinkingOverTheExpandedRangeReplacesTheHref() {
        val m = model("see [docs](https://a.example) now")
        val id = run(m).id
        val range = m.linkRangeAt(id, 6..6)!!
        m.toggleMark(id, range, InlineKind.Link, "https://b.example")
        assertEquals("see [docs](https://b.example) now", m.currentMarkdown())
    }

    @Test
    fun blankHrefOverTheLinkRangeRemovesTheLink() {
        val m = model("see [docs](https://a.example) now")
        val id = run(m).id
        val range = m.linkRangeAt(id, 6..6)!!
        m.toggleMark(id, range, InlineKind.Link, "")
        assertEquals("see docs now", m.currentMarkdown())
    }
    // --- host reload rule (EXP-655) -------------------------------------------

    private fun emitting(markdown: String): Pair<EditorModel, MutableList<String>> {
        val seen = mutableListOf<String>()
        val m = model(markdown)
        m.onEdit = { seen.add(m.currentMarkdown()) }
        return m to seen
    }

    @Test
    fun anEchoOfTheModelsOwnOutputNeverReloads() {
        val (m, seen) = emitting("Hello")
        m.updateRun(run(m).id, "Hello!", 6)
        assertFalse(m.reconcileHostMarkdown(seen.last()))
    }

    /** The race: the effect keyed on an OLDER echo runs after a newer keystroke. */
    @Test
    fun aStaleEchoTheModelHasMovedPastNeverReloads() {
        val (m, seen) = emitting("Hello")
        m.updateRun(run(m).id, "Hello!", 6)
        m.updateRun(run(m).id, "Hello!!", 7)
        assertEquals(2, seen.size)
        assertFalse(m.reconcileHostMarkdown(seen[0]))
        assertFalse(m.reconcileHostMarkdown(seen[1]))
        assertEquals("Hello!!", m.currentMarkdown())
    }

    @Test
    fun aValueTheModelNeverEmittedReloads() {
        val (m, _) = emitting("Hello")
        m.updateRun(run(m).id, "Hello!", 6)
        assertTrue(m.reconcileHostMarkdown("Someone else"))
    }

    /** History is cleared once the host caught up, so an old value coming back from remote is real. */
    @Test
    fun anOldEmissionReturningAfterTheHostCaughtUpReloads() {
        val (m, seen) = emitting("Hello")
        m.updateRun(run(m).id, "Hello!", 6)
        val old = seen.last()
        m.updateRun(run(m).id, "Hello!!", 7)
        assertFalse(m.reconcileHostMarkdown(seen.last())) // caught up
        assertTrue(m.reconcileHostMarkdown(old))
    }

    // --- focusEnd (EXP-655) -------------------------------------------------------

    @Test
    fun focusEndLandsOnTheLastRunAtItsEnd() {
        val m = model("Hello\n\nWorld")
        val last = runs(m).last()
        m.focusEnd()
        assertEquals(last.id, m.focusedRowId)
        assertEquals(last.text.length, m.consumeDesiredSelection(last.id))
    }

    @Test
    fun focusEndAfterATrailingImageUsesTheNormalizedEmptyRun() {
        val m = model("text\n\n![a](https://x/a.png)")
        val last = m.rows.last()
        assertTrue(last is EditorRow.TextRun)
        val before = m.revision(last.id)
        m.focusEnd()
        assertEquals(last.id, m.focusedRowId)
        assertEquals(0, m.consumeDesiredSelection(last.id))
        assertTrue(m.revision(last.id) > before)
        assertFalse(m.isDirty)
    }

    // --- Table cells (EXP-726). A cell is a field like any other: its edits
    // arrive through updateRun keyed by the CELL id, which the model routes
    // into the table. No manipulation UI exists on mobile, so these are the
    // only table mutations. ---

    private fun table(m: EditorModel) = m.rows.filterIsInstance<EditorRow.Table>().single().table

    @Test
    fun editingACellRewritesTheTable() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        val cell = table(m).rows[0][1]
        m.updateRun(cell.id, "22", 2)
        assertEquals("| a | b |\n| --- | --- |\n| 1 | 22 |", m.currentMarkdown())
        assertTrue(m.isDirty)
    }

    @Test
    fun editingAHeaderCellRewritesTheHeaderRow() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        m.updateRun(table(m).header[0].id, "name", 4)
        assertEquals("| name | b |\n| --- | --- |\n| 1 | 2 |", m.currentMarkdown())
    }

    @Test
    fun updateTableCellAddressesTheGridByCoordinate() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        val rowId = m.rows.filterIsInstance<EditorRow.Table>().single().id
        m.updateTableCell(rowId, row = 0, col = 1, text = "head")
        m.updateTableCell(rowId, row = 1, col = 0, text = "one")
        assertEquals("| a | head |\n| --- | --- |\n| one | 2 |", m.currentMarkdown())
    }

    /** A cell is ONE inline paragraph: a pasted newline folds to a space. */
    @Test
    fun aNewlinePastedIntoACellBecomesASpace() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        val cell = table(m).rows[0][0]
        m.updateRun(cell.id, "one\ntwo", 7)
        assertEquals("one two", table(m).rows[0][0].text)
        assertEquals("| a | b |\n| --- | --- |\n| one two | 2 |", m.currentMarkdown())
    }

    /** A `|` typed into a cell is escaped on the way out, never on the way in. */
    @Test
    fun aPipeTypedIntoACellIsEscapedOnSerialize() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        m.updateRun(table(m).rows[0][0].id, "x | y", 5)
        assertEquals("x | y", table(m).rows[0][0].text)
        assertEquals("| a | b |\n| --- | --- |\n| x \\| y | 2 |", m.currentMarkdown())
    }

    @Test
    fun cellMarksAreRemappedAroundAnInsertion() {
        val m = model("| a **b** | c |\n| --- | --- |\n| 1 | 2 |")
        val cell = table(m).header[0]
        assertEquals("a b", cell.text)
        m.updateRun(cell.id, "xa b", 1)
        val mark = table(m).header[0].marks.single()
        assertEquals(3, mark.start)
        assertEquals(4, mark.end)
        assertEquals("| xa **b** | c |\n| --- | --- |\n| 1 | 2 |", m.currentMarkdown())
    }

    /** No revision bump: the field already holds what the user typed. */
    @Test
    fun aCellEditDoesNotBumpItsRevision() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        val cell = table(m).rows[0][0]
        val before = m.revision(cell.id)
        m.updateRun(cell.id, "11", 2)
        assertEquals(before, m.revision(cell.id))
    }

    @Test
    fun loadingATableBumpsEveryCellRevision() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        assertTrue(table(m).allCells.all { m.revision(it.id) > 0 })
    }

    @Test
    fun locateCellFindsHeaderAndBodyCoordinates() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        val rowId = m.rows.filterIsInstance<EditorRow.Table>().single().id
        val t = table(m)
        assertEquals(EditorModel.CellLocation(rowId, 0, 1), m.locateCell(t.header[1].id))
        assertEquals(EditorModel.CellLocation(rowId, 1, 0), m.locateCell(t.rows[0][0].id))
        assertNull(m.locateCell("not-a-cell"))
    }

    /** BLOCK intents address ROWS; a cell has no block formatting to toggle. */
    @Test
    fun blockOpsNoOpForCellIds() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        val before = m.currentMarkdown()
        val cell = table(m).header[0]
        m.setHeading(cell.id, 1)
        m.toggleList(cell.id, ListType.Bullet)
        m.toggleQuote(cell.id)
        m.toggleCodeBlock(cell.id)
        m.clearParagraphFormat(cell.id, 0)
        assertEquals(before, m.currentMarkdown())
    }

    // --- Toolbar ops inside a cell. The toolbar stays up for a focused cell,
    // so every INLINE action has to reach it: marks over a selection, marks
    // queued at a collapsed caret, emoji/`@`/`#` insertion, clear formatting.
    // Only the block group above is a deliberate no-op. ---

    @Test
    fun boldOverASelectionInsideACell() {
        val m = model("| a | b |\n| --- | --- |\n| one | 2 |")
        val cell = table(m).rows[0][0]
        m.toggleMark(cell.id, 0..3, InlineKind.Bold)
        val mark = table(m).rows[0][0].marks.single()
        assertEquals(InlineKind.Bold, mark.kind)
        assertEquals(0, mark.start)
        assertEquals(3, mark.end)
        assertEquals("| a | b |\n| --- | --- |\n| **one** | 2 |", m.currentMarkdown())
    }

    /** Re-tapping Bold over the same covered range unwraps it again. */
    @Test
    fun boldTogglesOffAgainInsideACell() {
        val m = model("| a | b |\n| --- | --- |\n| **one** | 2 |")
        val cell = table(m).rows[0][0]
        m.toggleMark(cell.id, 0..3, InlineKind.Bold)
        assertTrue(table(m).rows[0][0].marks.isEmpty())
        assertEquals("| a | b |\n| --- | --- |\n| one | 2 |", m.currentMarkdown())
    }

    /** Bold at a collapsed caret then typing: the queue applies to the cell. */
    @Test
    fun pendingBoldMarksTheNextTextTypedInACell() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        val cell = table(m).rows[0][0]
        m.setFocused(cell.id)
        m.updateSelection(cell.id, 1..1)
        m.togglePendingMark(cell.id, 1, InlineKind.Bold)
        assertTrue(m.pendingMarkActive(cell.id, 1, InlineKind.Bold))
        m.updateRun(cell.id, "1x", 2)
        val mark = table(m).rows[0][0].marks.single()
        assertEquals(InlineKind.Bold, mark.kind)
        assertEquals(1, mark.start)
        assertEquals(2, mark.end)
        assertEquals("| a | b |\n| --- | --- |\n| 1**x** | 2 |", m.currentMarkdown())
    }

    @Test
    fun emojiInsertLandsAtTheCaretOfTheFocusedCell() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        val cell = table(m).rows[0][0]
        m.setFocused(cell.id)
        m.updateSelection(cell.id, 1..1)
        m.insertPlainText("🚀")
        assertEquals("1🚀", table(m).rows[0][0].text)
        assertEquals("| a | b |\n| --- | --- |\n| 1🚀 | 2 |", m.currentMarkdown())
    }

    /** `@` in a cell also arms the mention menu, exactly as it does in a run. */
    @Test
    fun mentionInsertArmsTheAutocompleteOnACell() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        val cell = table(m).rows[0][0]
        m.setFocused(cell.id)
        m.updateSelection(cell.id, 1..1)
        m.insertPlainText("@")
        assertEquals("1@", table(m).rows[0][0].text)
        assertTrue(m.consumeAutocompleteArm(cell.id))
    }

    @Test
    fun clearFormattingStripsMarksOverACellSelection() {
        val m = model("| a | b |\n| --- | --- |\n| **one** | 2 |")
        val cell = table(m).rows[0][0]
        m.clearFormatting(cell.id, 0..3)
        assertTrue(table(m).rows[0][0].marks.isEmpty())
        assertEquals("| a | b |\n| --- | --- |\n| one | 2 |", m.currentMarkdown())
    }

    /**
     * The image button with a cell focused: GFM has no block content inside a
     * cell, so the image lands right AFTER the table — never at the end of the
     * document past everything that followed it.
     */
    @Test
    fun imageInsertedFromACellLandsAfterTheTable() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |\n\ntail")
        val cell = table(m).header[0]
        m.setFocused(cell.id)
        m.insertImageUrl("/api/attachments/x", alt = "img")
        val tableIdx = m.rows.indexOfFirst { it is EditorRow.Table }
        val imageIdx = m.rows.indexOfFirst { it is EditorRow.Image }
        val tailIdx = m.rows.indexOfFirst { it is EditorRow.TextRun && it.text == "tail" }
        assertTrue(imageIdx > tableIdx)
        assertTrue(imageIdx < tailIdx)
        assertEquals(
            "| a | b |\n| --- | --- |\n| 1 | 2 |\n\n![img](/api/attachments/x)\n\ntail",
            m.currentMarkdown(),
        )
    }

    /** A table with nothing after it still gets a caret line under the image. */
    @Test
    fun imageInsertedFromACellInATrailingTable() {
        val m = model("| a | b |\n| --- | --- |\n| 1 | 2 |")
        m.setFocused(table(m).header[0].id)
        m.insertImageUrl("/api/attachments/x", alt = "img")
        assertEquals(
            "| a | b |\n| --- | --- |\n| 1 | 2 |\n\n![img](/api/attachments/x)",
            m.currentMarkdown(),
        )
        assertTrue(m.rows.last() is EditorRow.TextRun)
    }

    /** An ordered list restarts after a table, exactly as it does after an image. */
    @Test
    fun orderedListRestartsAfterATable() {
        val m = model("1. one\n\n| h |\n| --- |\n| x |\n\n1. two")
        val runs = runs(m).filter { it.text.isNotEmpty() }
        val last = runs.last()
        // A structural edit (a new line) is what triggers the renumber pass.
        m.updateRun(last.id, "two\nthree", 9)
        val after = runs(m).first { it.id == last.id }
        assertEquals(listOf(1, 2), after.paragraphs.map { it.orderedIndex })
        assertEquals(listOf(1), runs(m).first { it.id == runs.first().id }.paragraphs.map { it.orderedIndex })
    }
}
