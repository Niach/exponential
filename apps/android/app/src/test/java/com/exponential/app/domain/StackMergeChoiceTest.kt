package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1145: the stack merge dialog, locked x4 (web `pr-stack.test.ts`, iOS
 * `PrStackTests`, desktop `pr_stack`) against the ONE contract fixture
 * `stack-merge-choice.json`.
 */
class StackMergeChoiceTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("stack-merge-choice.json")).jsonObject

    private fun JsonObject.string(key: String): String? = get(key)?.jsonPrimitive?.contentOrNull

    private fun row(json: JsonObject) = IssueEntity(
        id = json.string("id")!!,
        boardId = "board-1",
        number = 1,
        identifier = json.string("identifier")!!,
        title = "Issue ${json.string("identifier")}",
        status = "backlog",
        priority = "none",
        sortOrder = 1.0,
        branch = json.string("branch"),
        prBaseBranch = json.string("prBaseBranch"),
        prState = json.string("prState"),
        prUrl = json.string("prUrl"),
        createdAt = "2026-09-10T10:00:00Z",
        updatedAt = "2026-09-10T10:00:00Z",
    )

    @Test
    fun `labels are byte exact`() {
        val labels = fixture.getValue("labels").jsonObject
        assertEquals(labels.string("title"), PrStack.STACK_MERGE_CHOICE_TITLE)
        assertEquals(labels.string("mergeStack"), PrStack.MERGE_STACK_LABEL)
        assertEquals(labels.string("mergeThis"), PrStack.MERGE_THIS_PR_LABEL)
        assertEquals(labels.string("cancel"), PrStack.STACK_MERGE_CANCEL_LABEL)
    }

    @Test
    fun `every fixture case matches`() {
        val cases = fixture.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val name = case.string("name")!!
            val issues = case.getValue("issues").jsonArray.map { row(it.jsonObject) }
            val issue = issues.first { it.id == case.string("issue") }
            val expectedJson = case.getValue("choice")
            val expected = if (expectedJson is JsonNull) null else expectedJson.jsonObject.let { choice ->
                PrStack.StackMergeChoice(
                    members = choice.getValue("members").jsonArray.map { it.jsonPrimitive.content },
                    position = choice.getValue("position").jsonPrimitive.int,
                    bottomIssueId = choice.string("bottomIssueId")!!,
                    topIssueId = choice.string("topIssueId")!!,
                    listing = choice.string("listing")!!,
                    stackSentence = choice.string("stackSentence")!!,
                    thisSentence = choice.string("thisSentence")!!,
                    body = choice.string("body")!!,
                )
            }
            assertEquals(name, expected, PrStack.stackMergeChoice(issue, issues))
        }
    }
}
