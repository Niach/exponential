package com.exponential.app.domain

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-785: the collapsed tool-group caption, locked ×4 (web
 * `tool-group-summary.test.ts`, iOS `ToolGroupSummaryTests`, desktop
 * `steer::tool_group_summary`) against the ONE contract fixture — same cases,
 * same test names.
 */
class ToolGroupSummaryTest {

    private data class FixtureCase(
        val name: String,
        val calls: List<ToolCallSummary>,
        val expected: String,
    )

    private fun cases(): List<FixtureCase> =
        Json.parseToJsonElement(fixtureJson()).jsonArray.map { element ->
            val case = element.jsonObject
            FixtureCase(
                name = case.getValue("name").jsonPrimitive.content,
                calls = case.getValue("calls").jsonArray.map { call ->
                    val obj = call.jsonObject
                    ToolCallSummary(
                        kind = obj.getValue("kind").jsonPrimitive.content,
                        detail = obj["detail"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content,
                        failed = obj.getValue("failed").jsonPrimitive.boolean,
                    )
                },
                expected = case.getValue("expected").jsonPrimitive.content,
            )
        }

    @Test
    fun `every fixture case renders byte exact`() {
        val cases = cases()
        assertTrue(cases.size >= 12)
        for (case in cases) {
            assertEquals(case.name, case.expected, ToolGroupSummary.summarize(case.calls))
        }
    }

    @Test
    fun `the fixture covers every segment`() {
        val expectations = cases().map { it.expected }
        fun covers(needle: String) = expectations.any { it.contains(needle) }
        assertTrue(covers("No tool calls"))
        assertTrue(covers("Used 1 tool"))
        assertTrue(covers("Used 3 tools"))
        for (first in listOf("Ran ", "Edited ", "Read ", "Searched ", "Fetched ")) {
            assertTrue(first, expectations.any { it.startsWith(first) })
        }
        for (segment in listOf(
            "1 command", "2 commands", "1 file", "2 files", "1 time", "2 times",
            "1 page", "2 pages", "1 other tool", "2 other tools", "1 failed", "2 failed",
        )) {
            assertTrue(segment, covers(segment))
        }
        assertEquals(" · ", ToolGroupSummary.SEPARATOR)
        assertTrue(covers(ToolGroupSummary.SEPARATOR))
    }
}

/**
 * The contract fixture, located relative to the Gradle test working directory
 * (the `app` module dir) with fallbacks so the suite also runs from the
 * `apps/android` dir or the repo root.
 */
private fun fixtureJson(): String {
    val candidates = listOf(
        "../../../packages/domain-contract/fixtures/tool-group-summary.json",
        "../../packages/domain-contract/fixtures/tool-group-summary.json",
        "packages/domain-contract/fixtures/tool-group-summary.json",
    )
    val file = candidates.map(::File).firstOrNull { it.isFile }
        ?: error("tool-group-summary.json not found from ${File(".").absolutePath}")
    return file.readText()
}
