package com.exponential.app.domain

import java.io.File as JavaFile
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-895: the fixture is the contract. Every mirror (TS
 * `domain-contract/src/diff.test.ts`, desktop `domain::diff`, iOS
 * `ExpCore/Sources/Domain/Diff.swift`) replays `fixtures/diff/cases.json` +
 * `summary.json` with THESE four test names, so a rule that moves there moves
 * everywhere or four suites go red at once.
 *
 * How a case is parsed (the fixture's own contract):
 *   - `form: "hunks"` WITH a `path` → `Diff.parsePatch(path, status, input)`
 *     (GitHub's PullFile shape: the path and status arrive beside the patch).
 *   - every other case, `form: "hunks"` WITHOUT a path included →
 *     `Diff.parse(input)`, which auto-detects the form. A pathless `hunks`
 *     case is exactly the auto-detect path: one file, empty path, `modified`.
 *   - `expected` is `render(…)`, `summary` is `summaryLabel(totals(…))`, and
 *     `unchanged` is the FIRST file's `[unchangedBefore(h0),
 *     unchangedBetween(h0, h1), …]` (empty when it has no hunks, or when
 *     there is no file at all).
 *   - `expectedMerged`, when present, is `render` over
 *     `mergeFilesByPath(files)`.
 */
class DiffTest {

    private data class FixtureCase(
        val name: String,
        val form: String,
        val input: String,
        val path: String?,
        val status: Diff.Status?,
        val expected: List<String>,
        val expectedMerged: List<String>?,
        val summary: String,
        val unchanged: List<Int>,
    )

    private data class SummaryRow(
        val files: Int,
        val additions: Int,
        val deletions: Int,
        val expected: String,
    )

    private fun cases(): List<FixtureCase> =
        Json.parseToJsonElement(fixtureJson("cases.json")).jsonArray.map { element ->
            val case = element.jsonObject
            fun text(key: String): String? =
                case[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content
            FixtureCase(
                name = case.getValue("name").jsonPrimitive.content,
                form = case.getValue("form").jsonPrimitive.content,
                input = case.getValue("input").jsonPrimitive.content,
                path = text("path"),
                status = text("status")?.let { Diff.Status.fromPullFile(it) },
                expected = case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                expectedMerged = case["expectedMerged"]?.takeUnless { it is JsonNull }
                    ?.jsonArray?.map { it.jsonPrimitive.content },
                summary = case.getValue("summary").jsonPrimitive.content,
                unchanged = case.getValue("unchanged").jsonArray.map { it.jsonPrimitive.int },
            )
        }

    private fun summaries(): List<SummaryRow> =
        Json.parseToJsonElement(fixtureJson("summary.json")).jsonArray.map { element ->
            val row = element.jsonObject
            SummaryRow(
                files = row.getValue("files").jsonPrimitive.int,
                additions = row.getValue("additions").jsonPrimitive.int,
                deletions = row.getValue("deletions").jsonPrimitive.int,
                expected = row.getValue("expected").jsonPrimitive.content,
            )
        }

    private fun parseCase(case: FixtureCase): Diff.Parsed {
        if (case.form == "hunks" && case.path != null) {
            return Diff.Parsed(
                listOf(
                    Diff.parsePatch(case.path, case.status ?: Diff.Status.MODIFIED, case.input)
                )
            )
        }
        return Diff.parse(case.input)
    }

    private fun unchangedRun(parsed: Diff.Parsed): List<Int> {
        val first = parsed.files.firstOrNull() ?: return emptyList()
        return first.hunks.mapIndexed { index, hunk ->
            if (index == 0) Diff.unchangedBefore(hunk)
            else Diff.unchangedBetween(first.hunks[index - 1], hunk)
        }
    }

    @Test
    fun `every fixture case parses byte exact`() {
        for (case in cases()) {
            val parsed = parseCase(case)
            assertEquals(case.name, case.expected, Diff.render(parsed))
            val merged = case.expectedMerged ?: continue
            assertEquals(
                "${case.name} (merged)",
                merged,
                Diff.render(parsed.copy(files = Diff.mergeFilesByPath(parsed.files))),
            )
        }
    }

    @Test
    fun `the fixture covers every input form`() {
        val cases = cases()
        assertTrue(cases.size >= 18)
        assertEquals(
            listOf("bare", "git", "hunks", "none"),
            cases.map { it.form }.toSortedSet().toList(),
        )
        // A `none` case is the empty parse; every other form yields files.
        for (case in cases) {
            val files = parseCase(case).files
            if (case.form == "none") assertEquals(case.name, emptyList<Diff.File>(), files)
            else assertTrue(case.name, files.isNotEmpty())
        }
    }

    @Test
    fun `summary label matches every fixture case`() {
        for (case in cases()) {
            val totals = Diff.totals(parseCase(case).files)
            assertEquals(
                case.name,
                case.summary,
                Diff.summaryLabel(totals.files, totals.additions, totals.deletions),
            )
        }
        val rows = summaries()
        assertTrue(rows.size >= 4)
        for (row in rows) {
            assertEquals(
                row.toString(),
                row.expected,
                Diff.summaryLabel(row.files, row.additions, row.deletions),
            )
        }
    }

    @Test
    fun `unchanged line counts match every fixture case`() {
        for (case in cases()) {
            assertEquals(case.name, case.unchanged, unchangedRun(parseCase(case)))
        }
    }

    @Test
    fun `mergeFilesByPath folds repeats and lets the later section win`() {
        val one = Diff.File(path = "b.ts", additions = 1, deletions = 1)
        val two = Diff.File(path = "a.ts", additions = 1, deletions = 1)
        val three = Diff.File(
            path = "b.ts",
            additions = 1,
            deletions = 1,
            status = Diff.Status.RENAMED,
            binary = true,
            previousPath = "old.ts",
        )
        val merged = Diff.mergeFilesByPath(listOf(one, two, three))
        // Order of FIRST appearance, and the repeat folds into it.
        assertEquals(listOf("b.ts", "a.ts"), merged.map { it.path })
        assertEquals(2, merged[0].additions)
        assertEquals(2, merged[0].deletions)
        assertEquals(Diff.Status.RENAMED, merged[0].status)
        assertTrue(merged[0].binary)
        assertEquals("old.ts", merged[0].previousPath)
    }

    @Test
    fun `mergeFilesByPath concatenates hunks and never mutates its input`() {
        val one = Diff.parsePatch("a.ts", Diff.Status.MODIFIED, "@@ -1 +1 @@\n-a\n+A\n")
        val two = Diff.parsePatch("a.ts", Diff.Status.MODIFIED, "@@ -9 +9 @@\n-b\n+B\n")
        val merged = Diff.mergeFilesByPath(listOf(one, two))
        assertEquals(1, merged.size)
        assertEquals(2, merged[0].hunks.size)
        assertEquals(1, one.hunks.size)
        assertEquals(1, one.additions)
    }

    @Test
    fun `an absent or empty patch is a file with no hunks`() {
        for (patch in listOf(null, "")) {
            val file = Diff.parsePatch("logo.png", Diff.Status.MODIFIED, patch)
            assertEquals(
                Diff.File(path = "logo.png", status = Diff.Status.MODIFIED),
                file,
            )
        }
    }

    @Test
    fun `the patch never renames the file it was handed`() {
        val file = Diff.parsePatch(
            "given.ts",
            Diff.Status.ADDED,
            "diff --git a/other.ts b/other.ts\n--- /dev/null\n+++ b/other.ts\n@@ -0,0 +1 @@\n+x\n",
        )
        assertEquals("given.ts", file.path)
        assertEquals(Diff.Status.ADDED, file.status)
        assertEquals(1, file.additions)
    }

    @Test
    fun `githubs status vocabulary maps onto ours`() {
        assertEquals(Diff.Status.ADDED, Diff.Status.fromPullFile("added"))
        assertEquals(Diff.Status.REMOVED, Diff.Status.fromPullFile("removed"))
        assertEquals(Diff.Status.MODIFIED, Diff.Status.fromPullFile("modified"))
        assertEquals(Diff.Status.RENAMED, Diff.Status.fromPullFile("renamed"))
        assertEquals(Diff.Status.COPIED, Diff.Status.fromPullFile("copied"))
        assertEquals(Diff.Status.MODIFIED, Diff.Status.fromPullFile("changed"))
        assertEquals(Diff.Status.MODIFIED, Diff.Status.fromPullFile("unchanged"))
        assertEquals(Diff.Status.MODIFIED, Diff.Status.fromPullFile("something-new"))
        assertEquals(
            listOf("added", "removed", "modified", "renamed", "copied"),
            Diff.Status.entries.map { it.wire },
        )
    }

    @Test
    fun `additions deletions and unchanged labels`() {
        assertEquals("+0", Diff.additionsLabel(0))
        assertEquals("+12", Diff.additionsLabel(12))
        // U+2212 MINUS SIGN, never an ASCII hyphen.
        assertEquals("−4", Diff.deletionsLabel(4))
        assertEquals(0x2212, Diff.deletionsLabel(4)[0].code)
        assertEquals("1 unchanged line", Diff.unchangedLabel(1))
        assertEquals("12 unchanged lines", Diff.unchangedLabel(12))
    }

    @Test
    fun `the summary label`() {
        assertEquals("No changes", Diff.summaryLabel(0, 0, 0))
        assertEquals("No changes", Diff.summaryLabel(0, 9, 9))
        assertEquals("1 file +2 −0", Diff.summaryLabel(1, 2, 0))
        assertEquals("3 files +12 −4", Diff.summaryLabel(3, 12, 4))
    }

    @Test
    fun `unchanged runs never go negative and never wrap`() {
        fun hunk(newStart: Int, newLines: Int) =
            Diff.Hunk(newStart, newLines, newStart, newLines, "", emptyList())
        assertEquals(0, Diff.unchangedBefore(hunk(1, 3)))
        assertEquals(0, Diff.unchangedBefore(hunk(0, 0)))
        assertEquals(39, Diff.unchangedBefore(hunk(40, 3)))
        assertEquals(16, Diff.unchangedBetween(hunk(1, 3), hunk(20, 3)))
        assertEquals(0, Diff.unchangedBetween(hunk(1, 30), hunk(20, 3)))
        // Saturated counters would overflow a 32-bit sum; the gap stays 0.
        assertEquals(
            0,
            Diff.unchangedBetween(hunk(Diff.LINE_MAX, Diff.LINE_MAX), hunk(Diff.LINE_MAX, 1)),
        )
    }
}

/**
 * The contract fixture, located relative to the Gradle test working directory
 * (the `app` module dir) with fallbacks so the suite also runs from the
 * `apps/android` dir or the repo root.
 */
private fun fixtureJson(name: String): String {
    val candidates = listOf(
        "../../../packages/domain-contract/fixtures/diff/$name",
        "../../packages/domain-contract/fixtures/diff/$name",
        "packages/domain-contract/fixtures/diff/$name",
    )
    val file = candidates.map(::JavaFile).firstOrNull { it.isFile }
        ?: error("$name not found from ${JavaFile(".").absolutePath}")
    return file.readText()
}
