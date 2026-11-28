package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-2172: [CiPageView] builds the SAME genUI trees as web
 * `apps/web/src/lib/ci-page-view.ts`: `fixtures/ci-page-view.json` replayed
 * node for node (ids, props, slots, events, child order). #EXP-2214: the four
 * tabs and the repo page's Runs tab with its Segmented.
 */
class CiPageViewTest {

    private val golden: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("ci-page-view.json")).jsonObject }
    private val scenario: CiPage.Input by lazy {
        CiPageTest.input(Json.parseToJsonElement(contractFixtureJson("ci-page.json")).jsonObject.getValue("scenario").jsonObject)
    }

    private fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.contentOrNull

    private fun normalize(e: JsonElement): JsonElement = when (e) {
        is JsonObject -> JsonObject(e.mapValues { normalize(it.value) })
        is JsonArray -> JsonArray(e.map(::normalize))
        is JsonPrimitive -> if (!e.isString && e !is JsonNull) e.content.toDoubleOrNull()?.let(::JsonPrimitive) ?: e else e
    }

    @Test
    fun `the CI page tree is web's`() {
        val cases = golden.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        assertTrue(cases.any { it.containsKey("repositoryId") })
        for (case in cases) {
            val world = when (case.str("world")) {
                "empty" -> scenario.copy(runs = emptyList(), jobs = emptyList(), pullRequests = emptyList(), runners = emptyList(), trains = emptyList(), trainEvents = emptyList())
                else -> scenario
            }
            // A `repositoryId` case is the repo page's Runs tab (#EXP-2169;
            // #EXP-2214: with its own `tab`).
            if (case.containsKey("repositoryId") && case["repositoryId"] !is JsonNull) {
                val phone = case.getValue("phone").jsonPrimitive.boolean
                val tree = CiPageView.ciRepoRunsTree(world, case.str("repositoryId")!!, case.str("tab") ?: "train", phone)
                assertEquals(case.str("name"), normalize(case.getValue("tree")), normalize(tree))
                continue
            }
            val f = case.getValue("filter").jsonObject
            val state = CiPageView.State(
                tab = case.str("tab")!!,
                filter = CiPage.RunFilter(f.str("status") ?: "all", f.str("repositoryId"), f.str("source")),
                phone = case.getValue("phone").jsonPrimitive.boolean,
            )
            assertEquals(case.str("name"), normalize(case.getValue("tree")), normalize(CiPageView.ciPageTree(world, state)))
        }
    }
}
