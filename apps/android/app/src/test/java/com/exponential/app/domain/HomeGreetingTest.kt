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

// EXP-1253 (EXP-1259 D1): the Agent home, replayed off `composer-home.json`
// (web `home-greeting.test.ts`, same case names).
class HomeGreetingTest {
    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("composer-home.json")).jsonObject

    private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content
    private fun JsonObject.int(key: String): Int = getValue(key).jsonPrimitive.int

    @Test
    fun `homeGreeting replays every case`() {
        val cases = fixture.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val user = case.getValue("user").let { if (it is JsonNull) null else it.jsonPrimitive.content }
            assertEquals(
                case.string("name"),
                case.string("expected"),
                AgentHome.homeGreeting(case.string("local"), user),
            )
        }
    }

    @Test
    fun `the buckets, copy and geometry are the fixture's`() {
        val buckets = fixture.getValue("buckets").jsonArray.map { it.jsonObject }
        assertEquals(
            buckets.map {
                HomeGreetingBucket(
                    it.string("id"),
                    it.int("from"),
                    it.int("to"),
                    it.getValue("variants").jsonArray.map { v -> v.jsonPrimitive.content },
                )
            },
            AgentHome.BUCKETS,
        )
        assertEquals(fixture.string("placeholder"), AgentHome.PLACEHOLDER)
        assertEquals(fixture.int("dayStartsAtHour"), AgentHome.DAY_STARTS_AT_HOUR)
        val g = fixture.getValue("geometry").jsonObject
        assertEquals(g.int("mark"), AgentHome.MARK)
        assertEquals(g.int("markPhone"), AgentHome.MARK_PHONE)
        assertEquals(g.int("gap"), AgentHome.GAP)
        assertEquals(g.int("headline"), AgentHome.HEADLINE)
        assertEquals(g.int("headlinePhone"), AgentHome.HEADLINE_PHONE)
        assertEquals(g.int("headlineWeight"), AgentHome.HEADLINE_WEIGHT)
        assertEquals(g.int("headlineToCard"), AgentHome.HEADLINE_TO_CARD)
        assertEquals(g.int("composerToGettingStarted"), AgentHome.COMPOSER_TO_GETTING_STARTED)
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.string("home"), AgentHome.HOME_TEST_ID)
        assertEquals(ids.string("greeting"), AgentHome.GREETING_TEST_ID)
        assertEquals(ids.string("mark"), AgentHome.MARK_TEST_ID)
    }
}
