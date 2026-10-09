package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1248: the ONE stack merge confirm (it replaces the 3-way dialog),
 * locked x4 against `stack-merge-choice.json` `confirm` (web `pr-stack.test.ts`
 * "stackMergeConfirm", desktop `stack_merge_confirm_matches_the_fixture`, iOS
 * `StackMergeConfirmTests`).
 */
class StackMergeConfirmTest {

    private val confirm: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("stack-merge-choice.json")).jsonObject
            .getValue("confirm").jsonObject

    private fun JsonObject.string(key: String): String? = get(key)?.jsonPrimitive?.contentOrNull

    @Test
    fun `locks the words to the contract`() {
        val labels = confirm.getValue("labels").jsonObject
        assertEquals(labels.string("mergeStack"), PrStack.MERGE_STACK_LABEL)
        assertEquals(labels.string("mergeThrough"), PrStack.MERGE_THROUGH_LABEL)
        assertEquals(labels.string("cancel"), PrStack.STACK_CONFIRM_CANCEL_LABEL)
        assertEquals(DomainContract.diffUiMergeStack, PrStack.MERGE_STACK_LABEL)
        assertEquals(DomainContract.diffUiMergeThrough, PrStack.MERGE_THROUGH_LABEL)
    }

    @Test
    fun `stack_merge_confirm matches the fixture`() {
        val cases = confirm.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val name = case.string("name")!!
            val issues = case.getValue("issues").jsonArray.map { stackFixtureIssue(it.jsonObject) }
            val issue = issues.first { it.id == case.string("issue") }
            val mode = PrStack.StackConfirmMode.entries.first { it.wire == case.string("mode") }
            val expectedJson = case.getValue("confirm")
            val expected = if (expectedJson is JsonNull) null else expectedJson.jsonObject.let { c ->
                PrStack.StackMergeConfirm(
                    title = c.string("title")!!,
                    landing = c.getValue("landing").jsonArray.map { it.jsonPrimitive.content },
                    staysOpen = c.getValue("staysOpen").jsonArray.map { it.jsonPrimitive.content },
                    body = c.string("body")!!,
                    issueId = c.getValue("input").jsonObject.string("issueId")!!,
                )
            }
            assertEquals(name, expected, PrStack.stackMergeConfirm(issue, issues, mode))
        }
    }
}
