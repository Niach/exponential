package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-630: the estimate helpers, locked ×4 against
 * `domain-contract/fixtures/issue-estimate.json` — same cases, same test
 * names as the web (`issue-estimate.test.ts`), iOS (IssueEstimateTests) and
 * the desktop (domain::issue_estimate). Each fixture case's `name` is the
 * assertion message, the way IssueNestingTest consumes its fixture.
 */
class IssueEstimateTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("issue-estimate.json")).jsonObject

    @Test
    fun `ladders and t-shirt labels match the fixture`() {
        val scales = fixture.getValue("scales").jsonObject
            .mapValues { (_, ladder) -> ladder.jsonArray.map { it.jsonPrimitive.int } }
        assertEquals(scales, ISSUE_ESTIMATION_SCALES)
        assertEquals(
            fixture.getValue("tshirtLabels").jsonArray.map { it.jsonPrimitive.content },
            ISSUE_ESTIMATE_TSHIRT_LABELS,
        )
        assertEquals(fixture.getValue("noEstimate").jsonPrimitive.content, NO_ESTIMATE)
    }

    @Test
    fun `label`() {
        for (row in fixture.getValue("labels").jsonArray.map { it.jsonObject }) {
            val name = row.getValue("name").jsonPrimitive.content
            val scale = row.getValue("scale").jsonPrimitive.content
            val value = row.getValue("value").let { if (it is JsonNull) null else it.jsonPrimitive.int }
            assertEquals(name, row.getValue("label").jsonPrimitive.content, estimateLabel(value, scale))
            if (value != null) {
                assertEquals(name, row.getValue("short").jsonPrimitive.content, estimateShortLabel(value, scale))
            }
        }
    }

    @Test
    fun `picker`() {
        for (row in fixture.getValue("pickers").jsonArray.map { it.jsonObject }) {
            val name = row.getValue("name").jsonPrimitive.content
            val scale = row.getValue("scale").jsonPrimitive.content
            val current = row.getValue("current").let { if (it is JsonNull) null else it.jsonPrimitive.intOrNull }
            assertEquals(
                name,
                row.getValue("values").jsonArray.map { it.jsonPrimitive.int },
                estimatePickerValues(current, scale),
            )
        }
    }

    @Test
    fun `phrase`() {
        for (row in fixture.getValue("phrases").jsonArray.map { it.jsonObject }) {
            val name = row.getValue("name").jsonPrimitive.content
            val scale = row.getValue("scale").jsonPrimitive.content
            assertEquals(
                name,
                row.getValue("phrase").jsonPrimitive.content,
                estimateEventPhrase(row.getValue("payload").jsonObject, scale),
            )
        }
    }

    // The fixture's phrase cases all carry a payload object; a missing one
    // (an event row whose payload column is NULL) reads as cleared too.
    @Test
    fun `phrase without a payload reads as cleared`() {
        assertEquals("removed the estimate", estimateEventPhrase(null, "fibonacci"))
    }
}
