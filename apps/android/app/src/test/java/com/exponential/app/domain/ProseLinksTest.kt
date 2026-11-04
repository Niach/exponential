package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1674: replayed ×4 against `prose-links.json` (web `prose-links.test.ts`,
 * desktop `domain::prose_links`, iOS `ProseLinksTests`).
 */
class ProseLinksTest {
    private val fixture = Json.parseToJsonElement(contractFixtureJson("prose-links.json")).jsonObject

    @Test
    fun everyCaseRewritesTheSame() {
        val origin = fixture.getValue("origin").jsonPrimitive.content
        val team = fixture.getValue("team").jsonPrimitive.content
        val synced = fixture.getValue("synced").jsonObject.mapValues { it.value.jsonPrimitive.content }
        val cases = fixture.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                case.getValue("out").jsonPrimitive.content,
                ProseLinks.chipBareAppLinks(case.getValue("text").jsonPrimitive.content, origin, team) { kind, id ->
                    synced["${kind.wire}:$id"]
                },
            )
        }
    }

    @Test
    fun everyChipKindCaseReadsTheSame() {
        fixture.getValue("chipKindCases").jsonArray.forEach { element ->
            val case = element.jsonObject
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                case.getValue("kind").jsonPrimitive.contentOrNull,
                ProseLinks.chipKind(case.getValue("href").jsonPrimitive.content)?.wire,
            )
        }
    }
}
