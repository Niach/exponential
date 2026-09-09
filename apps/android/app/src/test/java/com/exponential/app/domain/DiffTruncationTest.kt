package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-786: the publisher's cut note is a footer, never a diff line — the same
 * cases (and the same names) as web `agent-feed.test.ts`
 * "per-call diff truncation".
 */
class DiffTruncationTest {

    private val diff = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b"

    @Test
    fun `splits the trailing truncation line off`() {
        val cut = splitTruncatedDiff("$diff\n\\ 120 more lines truncated")
        assertEquals(diff, cut.diff)
        assertEquals(120, cut.truncated)

        val one = splitTruncatedDiff("$diff\n\\ 1 more line truncated\n")
        assertEquals(diff, one.diff)
        assertEquals(1, one.truncated)
    }

    @Test
    fun `leaves an uncut diff alone, no-newline markers included`() {
        val eof = "$diff\n\\ No newline at end of file"
        val cut = splitTruncatedDiff(eof)
        assertEquals(eof, cut.diff)
        assertNull(cut.truncated)
    }

    @Test
    fun `words the footer`() {
        assertEquals("1 more line truncated", diffTruncationNote(1))
        assertEquals("120 more lines truncated", diffTruncationNote(120))
    }

    /** Java's `$` matches before a final line terminator and JS's does not —
     *  the anchor is `\z`, so a note the publisher did NOT put last stays part
     *  of the diff instead of silently swallowing everything after it. */
    @Test
    fun `only a note at the very end is a footer`() {
        val trailing = "$diff\n\\ 5 more lines truncated\n+c"
        val cut = splitTruncatedDiff(trailing)
        assertEquals(trailing, cut.diff)
        assertNull(cut.truncated)
    }
}
