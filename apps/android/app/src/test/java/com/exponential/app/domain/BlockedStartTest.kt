package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
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
 * contract fixture `blocked-start.json`: every line, plan and prompt case,
 * the copy, the templates and the notes, byte for byte.
 */
class BlockedStartTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("blocked-start.json")).jsonObject

    private fun JsonObject.stringOrNull(key: String): String? =
        get(key)?.takeIf { it !is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content

    private fun JsonObject.strings(key: String): List<String> =
        getValue(key).jsonArray.map { it.jsonPrimitive.content }

    private fun cases(key: String): List<JsonObject> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    private fun issueRow(id: String, identifier: String, status: String) = IssueEntity(
        id = id,
        boardId = "board-1",
        number = 1,
        identifier = identifier,
        title = "Issue $identifier",
        status = status,
        priority = "none",
        sortOrder = 1.0,
        createdAt = "2026-10-02T10:00:00Z",
        updatedAt = "2026-10-02T10:00:00Z",
    )

    private fun base(json: JsonObject?): BlockedStart.Base? =
        json?.let { BlockedStart.Base(it.string("identifier"), it.string("branch")) }

    private fun JsonObject.objOrNull(key: String): JsonObject? =
        get(key)?.takeIf { it !is JsonNull }?.jsonObject

    @Test
    fun `locks the copy, maxRun and the templates`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.string("title"), BlockedStart.TITLE)
        assertEquals(copy.string("batchTitle"), BlockedStart.BATCH_TITLE)
        assertEquals(copy.string("batchBody"), BlockedStart.BATCH_BODY)
        assertEquals(copy.string("bodyPrefix"), BlockedStart.BODY_PREFIX)
        assertEquals(copy.string("bodySuffix"), BlockedStart.BODY_SUFFIX)
        assertEquals(copy.string("bodySuffixStackable"), BlockedStart.BODY_SUFFIX_STACKABLE)
        assertEquals(copy.string("startAnyway"), BlockedStart.START_ANYWAY)
        assertEquals(copy.string("stackedPr"), BlockedStart.STACKED_PR)
        assertEquals(fixture.getValue("maxRun").jsonPrimitive.int, BlockedStart.MAX_RUN)
        assertEquals(fixture.string("planNoteTemplate"), BlockedStart.PLAN_NOTE_TEMPLATE)
        assertEquals(fixture.string("baseTemplate"), BlockedStart.BASE_TEMPLATE)
        assertEquals(fixture.string("lineTemplate"), BlockedStart.LINE_TEMPLATE)
        assertEquals(fixture.string("textTemplate"), BlockedStart.TEXT_TEMPLATE)
    }

    @Test
    fun `locks the reasons, their order and their notes`() {
        assertEquals(fixture.strings("reasons"), BlockedStart.Reason.entries.map { it.wire })
        val notes = fixture.getValue("notes").jsonObject
        assertEquals(notes.keys, BlockedStart.Reason.entries.map { it.wire }.toSet())
        for (reason in BlockedStart.Reason.entries) {
            val expected = notes.string(reason.wire).replace("{ident}", "APP-7")
            assertEquals(reason.wire, expected, BlockedStart.stackDisabledNote(reason, "APP-7"))
        }
    }

    @Test
    fun `runs every line case`() {
        cases("lineCases").forEachIndexed { index, case ->
            val name = case.string("name")
            val issues = case.getValue("issues").jsonArray.map { row ->
                val obj = row.jsonObject
                issueRow(obj.string("id"), obj.string("identifier"), obj.string("status"))
            }
            val relations = case.getValue("relations").jsonArray.mapIndexed { row, relation ->
                val obj = relation.jsonObject
                relationRow(
                    id = "r$index-$row",
                    type = obj.string("type"),
                    issueId = obj.string("issueId"),
                    relatedIssueId = obj.string("relatedIssueId"),
                )
            }
            val line = BlockedStart.stackLine(case.string("subject"), relations, issues)
            assertEquals(name, case.strings("line"), line.line.map { it.identifier })
            assertEquals(name, case.stringOrNull("fork"), line.fork?.identifier)
            assertEquals(name, case.getValue("cycle").jsonPrimitive.boolean, line.cycle)
        }
    }

    @Test
    fun `runs every plan case`() {
        for (case in cases("planCases")) {
            val name = case.string("name")
            val subject = case.getValue("subject").jsonObject
            val line = case.getValue("line").jsonArray.map { row ->
                val obj = row.jsonObject
                BlockedStart.Member(
                    identifier = obj.string("identifier"),
                    prState = obj.stringOrNull("prState"),
                    branch = obj.stringOrNull("branch"),
                    repositoryId = obj.stringOrNull("repositoryId"),
                    running = obj.getValue("running").jsonPrimitive.boolean,
                )
            }
            val result = BlockedStart.stackPlan(
                pickedCount = case.getValue("pickedCount").jsonPrimitive.int,
                subject = BlockedStart.Subject(subject.string("identifier"), subject.stringOrNull("repositoryId")),
                line = line,
                fork = case.stringOrNull("fork"),
                cycle = case.getValue("cycle").jsonPrimitive.boolean,
            )
            val expectedPlan = case.objOrNull("plan")?.let { plan ->
                BlockedStart.Plan(base(plan.objOrNull("base")), plan.strings("run"))
            }
            assertEquals(name, expectedPlan, result.plan)
            assertEquals(name, case.stringOrNull("reason"), result.reason?.wire)
            assertEquals(name, case.stringOrNull("note"), result.note)
            val planNote = result.plan?.let { BlockedStart.stackPlanNote(it.run) }
            assertEquals(name, case.stringOrNull("planNote"), planNote)
        }
    }

    @Test
    fun `runs every prompt case`() {
        for (case in cases("promptCases")) {
            val name = case.string("name")
            val plan = BlockedStart.Plan(base(case.objOrNull("base")), case.strings("run"))
            assertEquals(name, case.string("prompt"), BlockedStart.stackedStartPrompt(plan, case.string("text")))
        }
    }

    // Identifiers are only team-unique: two teams with an `APP` board both
    // own an `APP-20`. `run[0]` resolves inside the walked line + subject.
    @Test
    fun `a stacked start resolves its first issue inside the walked line`() {
        val foreign = issueRow("team-b-app-20", "APP-20", "backlog")
        val bottom = issueRow("team-a-app-20", "APP-20", "backlog")
        val subject = issueRow("team-a-app-21", "APP-21", "backlog")
        val plan = BlockedStart.Plan(base = null, run = listOf("APP-20", "APP-21"))

        // What the old lookup did: the first identifier match over every team.
        assertEquals(foreign.id, listOf(foreign, bottom, subject).first { it.identifier == "APP-20" }.id)
        assertEquals(bottom.id, BlockedStart.stackStartIssueId(plan, listOf(bottom), subject))
        // A run of one starts the subject itself.
        assertEquals(
            subject.id,
            BlockedStart.stackStartIssueId(BlockedStart.Plan(null, listOf("APP-21")), emptyList(), subject),
        )
        // Nothing in the walk carries `run[0]`, or no plan: no start.
        assertEquals(null, BlockedStart.stackStartIssueId(plan, emptyList(), subject))
        assertEquals(null, BlockedStart.stackStartIssueId(null, listOf(bottom), subject))
    }
}
