package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1094: the Reviews row's ONE merge control, replayed from
 * `reviews-merge.json` by case name (web `reviews-merge.test.ts`, iOS,
 * desktop the same).
 */
class ReviewsMergeTest {
    private val fixture = Json.parseToJsonElement(contractFixtureJson("reviews-merge.json")).jsonObject

    private fun JsonElement?.str(): String? = (this as? JsonPrimitive)?.takeIf { it.isString }?.content

    @Test
    fun `labels and reasons are byte exact`() {
        val labels = fixture.getValue("labels").jsonObject
        assertEquals(labels["merge"].str(), ReviewsMerge.MERGE_LABEL)
        assertEquals(labels["mergeStack"].str(), ReviewsMerge.MERGE_STACK_LABEL)
        val reasons = fixture.getValue("reasons").jsonObject
        assertEquals(reasons["mergesWithStack"].str(), ReviewsMerge.MERGES_WITH_STACK)
        assertEquals(reasons["mergesThroughWorkflow"].str(), ReviewsMerge.MERGES_THROUGH_WORKFLOW)
    }

    @Test
    fun `every fixture case picks its action and reason`() {
        val cases = fixture.getValue("cases").jsonArray
        assertTrue(cases.size >= 13)
        cases.forEach { element ->
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val raw = case.getValue("input").jsonObject
            val stackWire = raw.getValue("stack").jsonPrimitive.content
            val input = ReviewMergeInput(
                stack = ReviewStackPosition.entries.first { it.wire == stackWire },
                workflowStatus = raw["workflowStatus"].str(),
                finalPr = raw.getValue("finalPr").jsonPrimitive.boolean,
                finalPrState = raw["finalPrState"].str(),
            )
            assertEquals(name, case["action"].str(), ReviewsMerge.reviewRowMergeAction(input).wire)
            assertEquals(name, case["disabledReason"].str(), ReviewsMerge.reviewsMergeDisabledReason(input))
        }
    }

    @Test
    fun `a live workflow wins the issue over a finished one`() {
        val byIssue = ReviewsMerge.workflowStatusByIssue(
            workflows = listOf("w1" to DomainContract.wfStatusDone, "w2" to DomainContract.wfStatusRunning),
            nodes = listOf("w1" to listOf("i1", "i2"), "w2" to listOf("i2")),
        )
        assertEquals(DomainContract.wfStatusDone, byIssue["i1"])
        assertEquals(DomainContract.wfStatusRunning, byIssue["i2"])
        assertEquals(DomainContract.wfStatusRunning, ReviewsMerge.reviewWorkflowStatus(listOf("i1", "i2"), byIssue))
        assertEquals(null, ReviewsMerge.reviewWorkflowStatus(listOf("x"), byIssue))
    }
}
