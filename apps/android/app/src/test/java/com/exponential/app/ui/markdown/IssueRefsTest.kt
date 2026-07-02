package com.exponential.app.ui.markdown

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Mirrors apps/web/src/lib/issue-refs.test.ts so the Android token detection
 * stays byte-for-byte aligned with the web regex (the interchange rule: an
 * inline `#IDENTIFIER` is plain GFM text that only pills at render time).
 */
class IssueRefsTest {

    private fun ids(text: String): List<String> = IssueRefs.findAll(text).map { it.identifier }

    @Test
    fun extractsASingleReference() {
        assertEquals(listOf("MET-115"), ids("duplicate of #MET-115, closing"))
    }

    @Test
    fun extractsMultipleReferencesInOrder() {
        // findAll keeps every occurrence (render pass needs each range); the
        // web extractIssueRefs dedupes — resolution makes that irrelevant here.
        assertEquals(
            listOf("APP-1", "MET-22", "APP-1"),
            ids("see #APP-1 and #MET-22 (also #APP-1 again)"),
        )
    }

    @Test
    fun keepsIdentifiersAsWritten() {
        // Case normalization happens in IssueRefHandler.resolve, not findAll.
        assertEquals(listOf("met-115"), ids("relates to #met-115"))
    }

    @Test
    fun matchesAtStartOfTextAndStartOfLine() {
        assertEquals(listOf("MET-1"), ids("#MET-1 first"))
        assertEquals(listOf("MET-2"), ids("line one\n#MET-2 second"))
    }

    @Test
    fun ignoresTokensGluedToAPrecedingWordOrHash() {
        assertEquals(emptyList<String>(), ids("foo#MET-115"))
        assertEquals(emptyList<String>(), ids("##MET-115"))
    }

    @Test
    fun ignoresTokensThatContinuePastTheNumber() {
        assertEquals(emptyList<String>(), ids("#MET-115abc"))
        assertEquals(emptyList<String>(), ids("#MET-115-2"))
    }

    @Test
    fun ignoresPlainHashesHeadingsAndNonIdentifierTokens() {
        assertEquals(emptyList<String>(), ids("# Heading"))
        assertEquals(emptyList<String>(), ids("#123"))
        assertEquals(emptyList<String>(), ids("#MET-"))
        assertEquals(emptyList<String>(), ids("#-115"))
        assertEquals(emptyList<String>(), ids("no refs here"))
    }

    @Test
    fun allowsDigitsInsideThePrefixButNotAsItsFirstChar() {
        assertEquals(listOf("A2C-9"), ids("#A2C-9"))
    }

    @Test
    fun matchesWhenFollowedByPunctuation() {
        assertEquals(listOf("MET-3", "MET-4", "MET-5"), ids("(#MET-3), #MET-4. #MET-5!"))
    }

    @Test
    fun rangesCoverHashPlusIdentifier() {
        val match = IssueRefs.findAll("see #MET-1!").single()
        assertEquals(4, match.start)
        assertEquals(10, match.end)
        assertEquals("#MET-1", "see #MET-1!".substring(match.start, match.end))
    }

    @Test
    fun resolveIsCaseInsensitiveAndUnknownStaysNull() {
        val handler = IssueRefHandler(
            targets = mapOf("MET-115" to IssueRefTarget("issue-1", "MET-115")),
            onOpen = {},
        )
        assertEquals("issue-1", handler.resolve("met-115")?.issueId)
        assertEquals("issue-1", handler.resolve("MET-115")?.issueId)
        assertNull(handler.resolve("MET-999"))
    }
}
