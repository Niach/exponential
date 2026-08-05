package com.exponential.app.ui.markdown

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.LinkAnnotation
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Regression cover for the issue-detail crash (masterplan §9.3): Compose throws
 * at layout when two `LinkAnnotation`s overlap or when a link range spills past
 * the text. `annotate` must coerce every link range into the text and drop any
 * link that overlaps one already added, so no combination of markdown links +
 * render-only `#IDENTIFIER` pills can produce an illegal AnnotatedString.
 */
class MarkdownAnnotateTest {

    private fun refs(vararg ids: String) = IssueRefHandler(
        candidates = ids.map { IssueRefTarget("id-$it", it) },
        onOpen = {},
    )

    private fun refsTitled(vararg pairs: Pair<String, String>) = IssueRefHandler(
        candidates = pairs.map { (id, title) -> IssueRefTarget("id-$id", id, title) },
        onOpen = {},
    )

    private fun mentions(vararg members: Pair<String, String>) = MentionResolver(
        members.map { (name, email) -> MentionMember(name, email) },
    )

    /** No two link annotations in the built string may overlap — the exact invariant Compose enforces. */
    private fun assertNoOverlappingLinks(annotated: androidx.compose.ui.text.AnnotatedString) {
        val ranges = annotated.getLinkAnnotations(0, annotated.length)
        for (i in ranges.indices) {
            for (j in i + 1 until ranges.size) {
                val a = ranges[i]
                val b = ranges[j]
                val overlap = a.start < b.end && b.start < a.end
                assertTrue(
                    "link annotations [${a.start},${a.end}) and [${b.start},${b.end}) overlap",
                    !overlap,
                )
            }
        }
    }

    @Test
    fun plainTextReturnsNoAnnotations() {
        val result = annotate("just words", emptyList(), null)
        assertEquals("just words", result.text)
        assertEquals(0, result.getLinkAnnotations(0, result.length).size)
    }

    @Test
    fun issueRefPillBecomesOneLink() {
        val text = "closes #MET-1 now"
        val result = annotate(text, emptyList(), refs("MET-1"))
        assertEquals(1, result.getLinkAnnotations(0, result.length).size)
        assertNoOverlappingLinks(result)
    }

    @Test
    fun unresolvedRefStaysPlain() {
        val text = "see #MET-9 (unknown)"
        val result = annotate(text, emptyList(), refs("MET-1"))
        assertEquals(0, result.getLinkAnnotations(0, result.length).size)
    }

    @Test
    fun markdownLinkAndPillCoexistWithoutOverlap() {
        // "[docs](https://x) fixes #MET-1"
        val text = "docs fixes #MET-1"
        val marks = listOf(InlineMark(0, 4, InlineKind.Link, href = "https://x"))
        val result = annotate(text, marks, refs("MET-1"))
        // One markdown link + one pill, disjoint ranges.
        assertEquals(2, result.getLinkAnnotations(0, result.length).size)
        assertNoOverlappingLinks(result)
    }

    @Test
    fun pillOverlappingAMarkdownLinkIsDropped() {
        // The `#MET-1` token sits inside a markdown link span; it must NOT
        // become a second (overlapping) link annotation.
        val text = "#MET-1 docs"
        val marks = listOf(InlineMark(0, 11, InlineKind.Link, href = "https://x"))
        val result = annotate(text, marks, refs("MET-1"))
        assertEquals(1, result.getLinkAnnotations(0, result.length).size)
        assertNoOverlappingLinks(result)
    }

    @Test
    fun overlappingMarkdownLinksAreDeduped() {
        // A parser edge could emit two overlapping Link marks; without the
        // overlap guard both would be added and Compose would crash at layout.
        val text = "hello world"
        val marks = listOf(
            InlineMark(0, 7, InlineKind.Link, href = "https://a"),
            InlineMark(4, 11, InlineKind.Link, href = "https://b"),
        )
        val result = annotate(text, marks, null)
        assertEquals(1, result.getLinkAnnotations(0, result.length).size)
        assertNoOverlappingLinks(result)
    }

    @Test
    fun outOfRangeLinkMarkIsCoercedNotThrown() {
        val text = "short"
        val marks = listOf(InlineMark(2, 999, InlineKind.Link, href = "https://x"))
        val result = annotate(text, marks, null)
        val links = result.getLinkAnnotations(0, result.length)
        assertEquals(1, links.size)
        assertTrue("end coerced within text", links[0].end <= text.length)
        assertNoOverlappingLinks(result)
    }

    // --- EXP-307: chips show the whole issue title next to the short code. ---

    @Test
    fun resolvedRefChipAppendsTheIssueTitle() {
        val result = annotate(
            "closes #MET-1 now",
            emptyList(),
            refsTitled("MET-1" to "Fix login flow"),
        )
        assertEquals("closes #MET-1 Fix login flow now", result.text)
        val links = result.getLinkAnnotations(0, result.length)
        assertEquals(1, links.size)
        // The tappable chip covers the token AND the title.
        assertEquals("#MET-1 Fix login flow", result.text.substring(links[0].start, links[0].end))
        assertNoOverlappingLinks(result)
    }

    @Test
    fun blankTitleKeepsTheBareToken() {
        val text = "closes #MET-1 now"
        val result = annotate(text, emptyList(), refs("MET-1"))
        assertEquals(text, result.text)
    }

    @Test
    fun longChipTitlesAreTruncatedWithAnEllipsis() {
        val title = "x".repeat(80)
        val result = annotate("#MET-1", emptyList(), refsTitled("MET-1" to title))
        assertEquals("#MET-1 " + "x".repeat(59) + "…", result.text)
    }

    @Test
    fun marksAfterATitledChipAreRemappedOntoTheDisplayText() {
        // "see #MET-1 and **bold**" — the appended title shifts later offsets.
        val text = "see #MET-1 and bold"
        val marks = listOf(InlineMark(15, 19, InlineKind.Bold))
        val result = annotate(text, marks, refsTitled("MET-1" to "Fix it"))
        assertEquals("see #MET-1 Fix it and bold", result.text)
        assertNoOverlappingLinks(result)
    }

    @Test
    fun mentionAndTitledChipComposeOnOneLine() {
        val result = annotate(
            "@dev@example.com bug #MET-1 done",
            emptyList(),
            refsTitled("MET-1" to "Crash"),
            mentions("Ada" to "dev@example.com"),
        )
        assertEquals("@Ada bug #MET-1 Crash done", result.text)
        val links = result.getLinkAnnotations(0, result.length)
        assertEquals(1, links.size)
        assertEquals("#MET-1 Crash", result.text.substring(links[0].start, links[0].end))
        assertNoOverlappingLinks(result)
    }

    // --- REV2-42: `@email` mention pills (display-only, name substituted). ---

    @Test
    fun knownMentionRendersTheMemberName() {
        val result = annotate(
            "ping @dev@example.com now",
            emptyList(),
            null,
            mentions("Ada Lovelace" to "dev@example.com"),
        )
        assertEquals("ping @Ada Lovelace now", result.text)
        // Not a link — nothing to open, so it can never collide with one.
        assertEquals(0, result.getLinkAnnotations(0, result.length).size)
    }

    @Test
    fun unknownMentionStaysPlainText() {
        val text = "ping @nobody@example.com now"
        val result = annotate(text, emptyList(), null, mentions("Ada" to "dev@example.com"))
        assertEquals(text, result.text)
    }

    @Test
    fun mentionResolvesCaseInsensitively() {
        val result = annotate(
            "@DEV@Example.com",
            emptyList(),
            null,
            mentions("Ada" to "dev@example.com"),
        )
        assertEquals("@Ada", result.text)
    }

    @Test
    fun mentionInsideInlineCodeStaysPlain() {
        val text = "`@dev@example.com`"
        val marks = listOf(InlineMark(0, text.length, InlineKind.InlineCode))
        val result = annotate(text, marks, null, mentions("Ada" to "dev@example.com"))
        assertEquals(text, result.text)
    }

    // The name substitution shifts every later offset — marks, links and issue
    // pills after a mention must still land on the right characters.
    @Test
    fun spansAfterAMentionAreRemappedOntoTheDisplayText() {
        // "@dev@example.com bug #MET-1" → "@Ada bug #MET-1"
        val text = "@dev@example.com bug #MET-1"
        val marks = listOf(InlineMark(17, 20, InlineKind.Bold)) // "bug"
        val result = annotate(text, marks, refs("MET-1"), mentions("Ada" to "dev@example.com"))
        assertEquals("@Ada bug #MET-1", result.text)
        val links = result.getLinkAnnotations(0, result.length)
        assertEquals(1, links.size)
        assertEquals("#MET-1", result.text.substring(links[0].start, links[0].end))
        assertNoOverlappingLinks(result)
    }

    @Test
    fun markdownLinkAroundAMentionIsRemappedNotDropped() {
        // The mention sits inside a link span, so it stays plain — and the link
        // still covers exactly its own text.
        val text = "mail @dev@example.com here"
        val marks = listOf(InlineMark(5, 21, InlineKind.Link, href = "https://x"))
        val result = annotate(text, marks, null, mentions("Ada" to "dev@example.com"))
        assertEquals(text, result.text)
        val links = result.getLinkAnnotations(0, result.length)
        assertEquals(1, links.size)
        assertEquals("@dev@example.com", result.text.substring(links[0].start, links[0].end))
    }

    // --- EXP-423: Linear chips — status glyph over a hidden `#`. ---

    // A BUILTIN status: its color comes from the design tokens, so the JVM test
    // never reaches `parseColor` (android.graphics is not mocked here).
    private val inProgress =
        IssueStatusResolver.builtinDefaults.first { it.builtinKey == IssueStatus.InProgress }

    private fun refsWithStatus(id: String, title: String) = IssueRefHandler(
        candidates = listOf(IssueRefTarget("id-$id", id, title, inProgress)),
        onOpen = {},
    )

    @Test
    fun aResolvedChipCarriesItsStatusGlyphAndHidesTheHash() {
        val line = annotateLine("closes #MET-1 now", emptyList(), refsWithStatus("MET-1", "Fix login"))
        assertEquals("closes #MET-1 Fix login now", line.text.text)
        assertEquals(1, line.chips.size)
        val chip = line.chips[0]
        assertEquals(inProgress.iconName, chip.iconName)
        assertEquals(7, chip.tokenStart) // the `#`
        assertEquals("#MET-1 Fix login", line.text.text.substring(chip.start, chip.end))
        // Exactly one character is hidden, and no character moved.
        assertTrue(
            "the `#` is transparent",
            line.text.spanStyles.any {
                it.start == chip.tokenStart &&
                    it.end == chip.tokenStart + 1 &&
                    it.item.color == Color.Transparent
            },
        )
    }

    @Test
    fun aChipWithoutAResolvedStatusKeepsItsHashVisible() {
        val line = annotateLine("closes #MET-1 now", emptyList(), refsTitled("MET-1" to "Fix login"))
        assertEquals(1, line.chips.size)
        assertNull(line.chips[0].iconName)
        assertTrue(line.text.spanStyles.none { it.item.color == Color.Transparent })
    }

    /** The visuals are spans + a painter now, so the link may not restyle them. */
    @Test
    fun theChipLinkCarriesNoStyles() {
        val line = annotateLine("closes #MET-1 now", emptyList(), refsWithStatus("MET-1", "Fix"))
        val link = line.text.getLinkAnnotations(0, line.text.length).single()
        assertNull((link.item as LinkAnnotation.Clickable).styles)
    }

    @Test
    fun anUnresolvedTokenProducesNoChipSpec() {
        val line = annotateLine("see #MET-9 (unknown)", emptyList(), refsWithStatus("MET-1", "Fix"))
        assertTrue(line.chips.isEmpty())
    }

    @Test
    fun stylesAndPillOnSameSpanDoNotCrash() {
        // Bold styling under a pill is allowed (styles + links may overlap);
        // only link-vs-link is illegal.
        val text = "bug #MET-1 here"
        val marks = listOf(InlineMark(4, 10, InlineKind.Bold))
        val result = annotate(text, marks, refs("MET-1"))
        assertEquals(1, result.getLinkAnnotations(0, result.length).size)
        assertNoOverlappingLinks(result)
    }
}
