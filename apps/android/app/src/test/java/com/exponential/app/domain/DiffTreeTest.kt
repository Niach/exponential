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
 * EXP-916: the fixture is the contract. Every mirror (TS
 * `domain-contract/src/diff-tree.test.ts`, web `@exp/ui` `FileDiffTree`,
 * desktop `domain::diff_tree`, iOS `DiffTree.swift`) replays
 * `fixtures/diff/tree.json` with THESE test names.
 *
 * A case: `files` (path + counts; status is irrelevant to the tree and
 * defaults to `modified`), an optional `query`, and `expected` =
 * `renderDiffTree(diffFileTree(files, query))`.
 */
class DiffTreeTest {

    private data class FixtureCase(
        val name: String,
        val files: List<Diff.File>,
        val query: String,
        val expected: List<String>,
    )

    private fun cases(): List<FixtureCase> =
        Json.parseToJsonElement(fixtureJson()).jsonArray.map { element ->
            val case = element.jsonObject
            FixtureCase(
                name = case.getValue("name").jsonPrimitive.content,
                files = case.getValue("files").jsonArray.map { row ->
                    val file = row.jsonObject
                    Diff.File(
                        path = file.getValue("path").jsonPrimitive.content,
                        additions = file.getValue("additions").jsonPrimitive.int,
                        deletions = file.getValue("deletions").jsonPrimitive.int,
                    )
                },
                query = case["query"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content ?: "",
                expected = case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
            )
        }

    private fun toFile(path: String, additions: Int = 1, deletions: Int = 0) =
        Diff.File(path = path, additions = additions, deletions = deletions)

    @Test
    fun `every fixture case renders byte exact`() {
        for (case in cases()) {
            assertEquals(
                case.name,
                case.expected,
                DiffTree.renderDiffTree(DiffTree.diffFileTree(case.files, case.query)),
            )
        }
    }

    @Test
    fun `the fixture covers a compaction, a query and an empty input`() {
        val names = cases().joinToString("\n") { it.name }
        assertTrue(names.contains("compacts"))
        assertTrue(names.contains("query"))
        assertTrue(names.contains("no files"))
    }

    @Test
    fun `a file node keeps its input index and full path`() {
        val files = listOf("src/b.ts", "src/a.ts", "top.ts").map { toFile(it) }
        val tree = DiffTree.diffFileTree(files)
        assertEquals(
            listOf(
                Triple(DiffTree.Kind.DIR, "src", -1),
                Triple(DiffTree.Kind.FILE, "top.ts", 2),
            ),
            tree.map { Triple(it.kind, it.path, it.index) },
        )
        assertEquals(
            listOf("src/a.ts" to 1, "src/b.ts" to 0),
            tree[0].children.map { it.path to it.index },
        )
    }

    @Test
    fun `a compacted directory's path is the deepest segment's`() {
        val tree = DiffTree.diffFileTree(listOf(toFile("apps/web/src/a.ts")))
        assertEquals("apps/web/src", tree[0].name)
        assertEquals("apps/web/src", tree[0].path)
    }
}

/**
 * The contract fixture, located relative to the Gradle test working directory
 * (the `app` module dir) with fallbacks so the suite also runs from the
 * `apps/android` dir or the repo root.
 */
private fun fixtureJson(): String {
    val candidates = listOf(
        "../../../packages/domain-contract/fixtures/diff/tree.json",
        "../../packages/domain-contract/fixtures/diff/tree.json",
        "packages/domain-contract/fixtures/diff/tree.json",
    )
    val file = candidates.map(::JavaFile).firstOrNull { it.isFile }
        ?: error("tree.json not found from ${JavaFile(".").absolutePath}")
    return file.readText()
}
