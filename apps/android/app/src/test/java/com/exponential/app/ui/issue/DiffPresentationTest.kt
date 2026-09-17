package com.exponential.app.ui.issue

import com.exponential.app.data.api.PullFile
import com.exponential.app.domain.Diff
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-895: the pure decisions the ONE diff view makes ABOUT a [Diff.File] —
 * everything the composables read before they draw. The strings are
 * byte-identical with iOS `DiffPresentation` and web `@exp/ui`
 * (`file-diff-card.tsx`, `file-diff-nav.tsx`, `changes-file-sheet.tsx`), so a
 * word that moves there moves here.
 */
class DiffPresentationTest {

    private val patch = """
        @@ -3,4 +3,5 @@ fun main() {
         context one
        -gone
        +added one
        +added two
         context two
    """.trimIndent()

    private fun file(path: String, additions: Int = 1, deletions: Int = 0) = Diff.File(
        path = path,
        status = Diff.Status.MODIFIED,
        additions = additions,
        deletions = deletions,
    )

    // ── The file-row labels ──────────────────────────────────────────────────

    @Test
    fun `the status letter is one per status`() {
        assertEquals("A", diffStatusLetter(Diff.Status.ADDED))
        assertEquals("D", diffStatusLetter(Diff.Status.REMOVED))
        assertEquals("M", diffStatusLetter(Diff.Status.MODIFIED))
        assertEquals("R", diffStatusLetter(Diff.Status.RENAMED))
        assertEquals("C", diffStatusLetter(Diff.Status.COPIED))
    }

    @Test
    fun `a path splits into its dimmed directory and its basename`() {
        assertEquals("apps/web/src/", diffPathDir("apps/web/src/router.tsx"))
        assertEquals("router.tsx", diffPathBase("apps/web/src/router.tsx"))
        // A repo-root file is all basename, no crumb.
        assertEquals("", diffPathDir("README.md"))
        assertEquals("README.md", diffPathBase("README.md"))
    }

    @Test
    fun `middle truncation keeps both ends of a long directory`() {
        val long = "apps/android/app/src/main/java/com/exponential/app/"
        val cut = middleTruncatePath(long, 22)
        assertEquals(22, cut.length)
        assertTrue(cut.startsWith("apps/andr"))
        assertTrue(cut.endsWith("app/"))
        assertTrue(cut.contains("…"))
        // Short enough to fit is left alone, byte for byte.
        assertEquals("ui/", middleTruncatePath("ui/", 22))
    }

    // ── What a file with no rows says ────────────────────────────────────────

    @Test
    fun `a file with no hunks says which kind of nothing it is`() {
        assertEquals("Binary file", noHunksNote(file("a.png").copy(binary = true)))
        assertEquals("Empty file added", noHunksNote(file("a.kt").copy(status = Diff.Status.ADDED)))
        assertEquals("File removed", noHunksNote(file("a.kt").copy(status = Diff.Status.REMOVED)))
        assertEquals(
            "Renamed without content changes",
            noHunksNote(file("a.kt").copy(status = Diff.Status.RENAMED)),
        )
        assertEquals(
            "Copied without content changes",
            noHunksNote(file("a.kt").copy(status = Diff.Status.COPIED)),
        )
        assertEquals("No textual diff (binary or too large)", noHunksNote(file("a.kt")))
    }

    // ── The rows a card draws ────────────────────────────────────────────────

    @Test
    fun `rows lead with the skipped context, then the header, then the lines`() {
        val hunks = Diff.parsePatch("a.kt", Diff.Status.MODIFIED, patch).hunks
        val built = buildDiffRows(hunks)
        assertEquals(0, built.hidden)
        // 2 unchanged lines above the first hunk (`newStart` 3), a plain row.
        assertEquals(DiffRow.Gap("2 unchanged lines"), built.rows[0])
        assertEquals("@@ -3,4 +3,5 @@ fun main() {", (built.rows[1] as DiffRow.Header).text)
        assertEquals(
            listOf(
                Diff.LineKind.CONTEXT,
                Diff.LineKind.DEL,
                Diff.LineKind.ADD,
                Diff.LineKind.ADD,
                Diff.LineKind.CONTEXT,
            ),
            built.rows.drop(2).map { (it as DiffRow.Body).line.kind },
        )
    }

    @Test
    fun `a gap divider counts the context between two hunks`() {
        val two = """
            @@ -1,2 +1,2 @@
            -a
            +b
            @@ -20,2 +20,2 @@
            -c
            +d
        """.trimIndent()
        val rows = buildDiffRows(Diff.parsePatch("a.kt", Diff.Status.MODIFIED, two).hunks).rows
        val gaps = rows.filterIsInstance<DiffRow.Gap>().map { it.text }
        // Nothing above the first hunk (it starts at line 1); 17 between them.
        assertEquals(listOf("17 unchanged lines"), gaps)
    }

    @Test
    fun `the cap stops at body rows and reports what did not fit`() {
        val hunks = Diff.parsePatch("a.kt", Diff.Status.MODIFIED, patch).hunks
        val built = buildDiffRows(hunks, maxLines = 2)
        assertEquals(2, built.rows.count { it is DiffRow.Body })
        assertEquals(3, built.hidden)
    }

    // ── What opens on its own ────────────────────────────────────────────────

    @Test
    fun `a review queue folds every card, a run's own output opens the small ones`() {
        val small = Diff.parsePatch("a.kt", Diff.Status.MODIFIED, patch)
        assertEquals(5, diffLineCount(small))
        assertTrue(diffOpensByDefault(small, defaultCollapsed = false))
        assertTrue(!diffOpensByDefault(small, defaultCollapsed = true))

        // Past the threshold a file stays shut even where cards open, so one
        // lockfile cannot bury every card under it.
        val huge = buildString {
            appendLine("@@ -1,400 +1,400 @@")
            repeat(400) { appendLine("+line $it") }
        }
        val big = Diff.parsePatch("lock", Diff.Status.MODIFIED, huge)
        assertTrue(diffLineCount(big) > 300)
        assertTrue(!diffOpensByDefault(big, defaultCollapsed = false))
    }

    // ── GitHub's PullFile ────────────────────────────────────────────────────

    @Test
    fun `a PullFile with a patch takes the parser's counts`() {
        val parsed = PullFile(
            filename = "a.kt",
            status = "modified",
            // Deliberately wrong: the rows under the header must agree with it.
            additions = 99,
            deletions = 99,
            patch = patch,
        ).toDiffFile()
        assertEquals("a.kt", parsed.path)
        assertEquals(Diff.Status.MODIFIED, parsed.status)
        assertEquals(2, parsed.additions)
        assertEquals(1, parsed.deletions)
    }

    @Test
    fun `a PullFile GitHub sent no patch for keeps GitHub's counts`() {
        val parsed = PullFile(
            filename = "big.lock",
            status = "added",
            additions = 4000,
            deletions = 0,
            patch = null,
        ).toDiffFile()
        assertTrue(parsed.hunks.isEmpty())
        assertEquals(Diff.Status.ADDED, parsed.status)
        assertEquals(4000, parsed.additions)
        assertEquals("Empty file added", noHunksNote(parsed))
    }
}
