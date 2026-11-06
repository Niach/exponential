package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// #EXP-1642: the phone's own search, replayed off `system-search.json`
// (TS `system-search.test.ts`, same case names).
class SystemSearchTest {
    private val fixture: JsonObject = Json.parseToJsonElement(contractFixtureJson("system-search.json")).jsonObject

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.flag(key: String): Boolean = this[key]?.jsonPrimitive?.booleanOrNull == true

    private fun cases(key: String): List<JsonObject> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    @Test
    fun `copy, prefixes, caps and debounce are the fixture's`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("issueSubtitle"), SystemSearch.ISSUE_SUBTITLE)
        assertEquals(copy.str("chatSubtitle"), SystemSearch.CHAT_SUBTITLE)
        assertEquals(fixture.str("issuePrefix"), SystemSearch.ISSUE_PREFIX)
        assertEquals(fixture.str("chatPrefix"), SystemSearch.CHAT_PREFIX)
        assertEquals(fixture.str("domainPrefix"), SystemSearch.DOMAIN_PREFIX)
        assertEquals(fixture.getValue("debounceMs").jsonPrimitive.long, SystemSearch.DEBOUNCE_MS)
        assertEquals(
            SystemSearch.Caps(fixture.getValue("maxIssues").jsonPrimitive.int, fixture.getValue("maxChats").jsonPrimitive.int),
            SystemSearch.FIXTURE_CAPS,
        )
        val production = fixture.getValue("productionCaps").jsonObject
        assertEquals(
            SystemSearch.Caps(production.getValue("maxIssues").jsonPrimitive.int, production.getValue("maxChats").jsonPrimitive.int),
            SystemSearch.PRODUCTION_CAPS,
        )
        assertEquals("u1", SystemSearch.accountOf(SystemSearch.domain("u1")))
        assertEquals(null, SystemSearch.accountOf("other"))
    }

    @Test
    fun `itemCases match the fixture`() {
        for (case in cases("itemCases")) {
            val input = case.getValue("input").jsonObject
            val boards = input.getValue("boards").jsonArray.map {
                val b = it.jsonObject
                SystemSearch.Board(b.str("id")!!, b.str("name")!!)
            }
            val issues = input.getValue("issues").jsonArray.map {
                val i = it.jsonObject
                SystemSearch.Issue(i.str("id")!!, i.str("identifier")!!, i.str("title")!!, i.str("boardId")!!, i.str("updatedAt")!!, i.flag("trashed"), i.flag("archived"))
            }
            val chats = input.getValue("chats").jsonArray.map {
                val c = it.jsonObject
                ShareTargets.Chat(c.str("id")!!, c.str("title")!!, c.str("updatedAt")!!, c.str("agentId"), c.flag("sealed"))
            }
            val expected = case.getValue("expected").jsonArray.map {
                val e = it.jsonObject
                SystemSearch.Item(e.str("id")!!, e.str("title")!!, e.str("subtitle")!!, e.getValue("keywords").jsonArray.map { k -> k.jsonPrimitive.content })
            }
            assertEquals(case.str("name"), expected, SystemSearch.systemSearchItems(boards, issues, chats))
        }
    }

    @Test
    fun `parseCases match the fixture`() {
        for (case in cases("parseCases")) {
            val got = SystemSearch.parseSystemSearchId(case.getValue("input").jsonPrimitive.content)
            val expected = case["expected"]?.takeUnless { it is JsonNull }?.jsonObject
            assertEquals(case.str("name"), expected?.str("kind"), got?.kind?.name?.lowercase())
            assertEquals(case.str("name"), expected?.str("id"), got?.id)
        }
    }
}
