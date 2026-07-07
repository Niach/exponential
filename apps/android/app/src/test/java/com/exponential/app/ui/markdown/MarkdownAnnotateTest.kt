package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark
import org.junit.Assert.assertEquals
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
