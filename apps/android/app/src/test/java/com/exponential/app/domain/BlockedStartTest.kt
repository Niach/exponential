package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * SLOP-3: the blocked-start dialog, locked x4 (web `blocked-start.test.ts`,
 * desktop `domain::blocked_start`, iOS `BlockedStartTests`) against the ONE
 * contract fixture `blocked-start.json`: every target case, every prompt
 * case and the copy, byte for byte.
 */
class BlockedStartTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("blocked-start.json")).jsonObject

    private fun JsonObject.stringOrNull(key: String): String? =
        get(key)?.takeIf { it !is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content

    @Test
    fun `locks the copy`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.string("title"), BlockedStart.TITLE)
        assertEquals(copy.string("batchTitle"), BlockedStart.BATCH_TITLE)
        assertEquals(copy.string("batchBody"), BlockedStart.BATCH_BODY)
        assertEquals(copy.string("bodyPrefix"), BlockedStart.BODY_PREFIX)
        assertEquals(copy.string("bodySuffix"), BlockedStart.BODY_SUFFIX)
        assertEquals(copy.string("bodySuffixStackable"), BlockedStart.BODY_SUFFIX_STACKABLE)
        assertEquals(copy.string("startAnyway"), BlockedStart.START_ANYWAY)
        assertEquals(copy.string("stackedPr"), BlockedStart.STACKED_PR)
    }

    @Test
    fun `locks the reasons, their order and their notes`() {
        val reasons = fixture.getValue("reasons").jsonArray.map { it.jsonPrimitive.content }
        assertEquals(reasons, BlockedStart.Reason.entries.map { it.wire })
        val notes = fixture.getValue("notes").jsonObject
        for (reason in BlockedStart.Reason.entries) {
            val expected = notes.string(reason.wire).replace("{ident}", "APP-7")
            assertEquals(reason.wire, expected, BlockedStart.stackDisabledNote(reason, "APP-7"))
        }
    }

    @Test
    fun `runs every target case`() {
        val cases = fixture.getValue("targetCases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val name = case.string("name")
            val blockers = case.getValue("blockers").jsonArray.map { row ->
                val obj = row.jsonObject
                BlockedStart.Blocker(
                    identifier = obj.string("identifier"),
                    prState = obj.stringOrNull("prState"),
                    branch = obj.stringOrNull("branch"),
                    repositoryId = obj.stringOrNull("repositoryId"),
                )
            }
            val result = BlockedStart.stackTarget(
                pickedCount = case.getValue("pickedCount").jsonPrimitive.int,
                subjectRepositoryId = case.stringOrNull("subjectRepositoryId"),
                blockers = blockers,
            )
            assertEquals(name, case.stringOrNull("target"), result.target?.identifier)
            assertEquals(name, case.stringOrNull("reason"), result.reason?.wire)
            val note = result.reason?.let {
                BlockedStart.stackDisabledNote(it, blockers.firstOrNull()?.identifier.orEmpty())
            }
            assertEquals(name, case.stringOrNull("note"), note)
        }
    }

    @Test
    fun `runs every prompt case`() {
        val template = fixture.string("promptTemplate")
        val cases = fixture.getValue("promptCases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val name = case.string("name")
            val identifier = case.string("identifier")
            val branch = case.string("branch")
            val prompt = BlockedStart.stackedStartPrompt(identifier, branch, case.string("text"))
            assertEquals(name, case.string("prompt"), prompt)
            // The template is the prompt's first paragraph.
            val base = template.replace("{ident}", identifier).replace("{branch}", branch)
            assertTrue(name, prompt.startsWith(base))
        }
    }
}
