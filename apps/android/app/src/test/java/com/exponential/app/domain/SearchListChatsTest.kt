package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EDE-99: the global search's Chats section is fixture-locked ×4: every
 * `sectionCases`, `rowCases`, `pickCases` and `startCases` case of
 * `search-list.json` replays here (TS `search-list.test.ts`, desktop
 * `domain::search_list`, iOS); the constants, copy and test ids are the fixture's.
 */
class SearchListChatsTest {
    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("search-list.json")).jsonObject }

    private fun cases(key: String): List<JsonObject> =
        (fixture[key] as JsonArray).map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    private fun JsonObject.str(key: String): String? = this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.name(): String = str("name").orEmpty()

    private fun hit(o: JsonObject) = SearchList.ChatHit(
        rootId = o.str("rootId")!!,
        sessionId = o.str("sessionId")!!,
        deviceId = o.str("deviceId"),
        kind = o.str("kind")!!,
        at = o.getValue("at").jsonPrimitive.long,
        seq = o["seq"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.long,
        messageId = o.str("messageId"),
        snippet = o.str("snippet")!!,
        matchStart = o.getValue("matchStart").jsonPrimitive.int,
        matchLen = o.getValue("matchLen").jsonPrimitive.int,
        system = o["system"]?.jsonPrimitive?.boolean == true,
    )

    @Test
    fun `constants copy and test ids match the fixture`() {
        val chats = fixture.getValue("chats").jsonObject
        assertEquals(chats.getValue("debounceMs").jsonPrimitive.long, SearchList.Chats.DEBOUNCE_MS)
        assertEquals(chats.getValue("chatsMax").jsonPrimitive.int, SearchList.Chats.CHATS_MAX)
        val row = chats.getValue("row").jsonObject
        assertEquals(row.getValue("chatHeight").jsonPrimitive.int, SearchList.Chats.ROW_HEIGHT)
        assertEquals(row.getValue("chatHeightPhone").jsonPrimitive.int, SearchList.Chats.ROW_HEIGHT_PHONE)
        val icons = chats.getValue("icons").jsonObject
        assertEquals(icons.str("chat"), SearchList.Chats.ICON_CHAT)
        assertEquals(icons.str("agent"), SearchList.Chats.ICON_AGENT)
        val copy = chats.getValue("copy").jsonObject
        val ours = mapOf(
            "issues" to SearchList.Chats.Copy.ISSUES,
            "chats" to SearchList.Chats.Copy.CHATS,
            "you" to SearchList.Chats.Copy.YOU,
            "noResults" to SearchList.Chats.Copy.NO_RESULTS,
            "placeholder" to SearchList.Chats.Copy.PLACEHOLDER,
        )
        assertEquals(copy.keys, ours.keys)
        for ((key, value) in ours) assertEquals(key, copy.str(key), value)
        val ids = chats.getValue("testIds").jsonObject
        assertEquals(ids.str("issues"), SearchList.Chats.TestIds.ISSUES)
        assertEquals(ids.str("chats"), SearchList.Chats.TestIds.CHATS)
        assertEquals(ids.str("chatRow"), SearchList.Chats.TestIds.CHAT_ROW)
    }

    /** #NEW-133: `suggestion` (the constants and every case). */
    @Test
    fun `suggestion replays the suggestion cases`() {
        val spec = fixture.getValue("suggestion").jsonObject
        assertEquals(spec.str("icon"), SearchList.Suggestion.ICON)
        assertEquals(spec.str("label"), SearchList.Suggestion.LABEL)
        assertEquals(spec.str("testId"), SearchList.Suggestion.TEST_ID)
        val cases = spec.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val members = case.getValue("members").jsonArray.map { it.jsonObject }.map {
                IssueSearchPhrase.Member(name = it.str("name"), email = it.str("email"))
            }
            val now = IssueSearch.epochMs(case.str("now"))
            val got = SearchList.suggestion(case.str("text")!!, members, now)
            val want = (case["expected"] as? JsonObject)?.let { SearchList.SearchSuggestion(it.str("query")!!, it.str("label")!!) }
            assertEquals(case.name(), want, got)
        }
    }

    @Test
    fun `globalSections replays sectionCases`() {
        for (case in cases("sectionCases")) {
            val input = case.getValue("input").jsonObject
            val chats = input["chats"]?.takeUnless { it is JsonNull }?.jsonObject?.let {
                SearchList.ChatsState(it.getValue("rows").jsonPrimitive.int, it.getValue("loading").jsonPrimitive.boolean)
            }
            val got = SearchList.globalSections(input.str("query")!!, input.getValue("issues").jsonPrimitive.int, chats)
            val want = case.getValue("expected").jsonObject
            val name = case.name()
            assertEquals(name, want.getValue("titled").jsonPrimitive.boolean, got.titled)
            assertEquals(name, want["count"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.int, got.count)
            assertEquals(name, want.getValue("empty").jsonPrimitive.boolean, got.empty)
            val sections = want.getValue("sections").jsonArray.map { it.jsonObject }.map {
                Triple(it.str("key"), it.getValue("count").jsonPrimitive.int, it.getValue("loading").jsonPrimitive.boolean)
            }
            assertEquals(name, sections, got.sections.map { Triple(it.key.wire, it.count, it.loading) })
        }
    }

    @Test
    fun `globalChatRows replays rowCases`() {
        for (case in cases("rowCases")) {
            val input = case.getValue("input").jsonObject
            val roots = input.getValue("roots").jsonObject.mapValues { (_, v) ->
                SearchList.ChatRoot(v.jsonObject.str("title")!!, v.jsonObject.getValue("agent").jsonPrimitive.boolean)
            }
            val authors = input.getValue("messageAuthors").jsonObject.mapValues { (_, v) -> v.jsonPrimitive.content }
            val chats = input.getValue("chats").jsonArray.map { it.jsonObject }.map {
                SearchList.ChatMatch(it.str("rootId")!!, it.getValue("count").jsonPrimitive.int, hit(it.getValue("hit").jsonObject))
            }
            val got = SearchList.globalChatRows(
                query = input.str("query")!!,
                now = input.str("now")!!,
                utcOffsetMinutes = input.getValue("utcOffsetMinutes").jsonPrimitive.int,
                chats = chats,
                roots = roots,
                messageAuthors = authors,
                limit = input.getValue("limit").jsonPrimitive.int,
            )
            val want = case.getValue("expected").jsonArray.map { it.jsonObject }
            assertEquals(case.name(), want.size, got.size)
            want.zip(got).forEach { (w, g) ->
                val name = "${case.name()} / ${w.str("key")}"
                assertEquals(name, w.str("key"), g.key)
                assertEquals(name, w.str("rootId"), g.rootId)
                assertEquals(name, w.str("icon"), g.icon)
                assertEquals(name, w.str("title"), g.title)
                assertEquals(name, w.str("subtitle"), g.subtitle)
                assertEquals(name, w.str("highlight"), g.highlight)
                assertEquals(name, w.str("trailing"), g.trailing)
            }
        }
    }

    @Test
    fun `globalChatPick replays pickCases`() {
        for (case in cases("pickCases")) {
            val input = case.getValue("input").jsonObject
            val got = SearchList.globalChatPick(input.str("query")!!, hit(input.getValue("hit").jsonObject))
            val want = case.getValue("expected").jsonObject
            val find = want.getValue("find").jsonObject
            val target = find.getValue("target").jsonObject
            assertEquals(case.name(), want.str("sessionId"), got.sessionId)
            assertEquals(case.name(), find.str("query"), got.query)
            assertEquals(case.name(), SearchList.ChatTarget(target.str("key")!!, target.getValue("output").jsonPrimitive.boolean), got.target)
        }
    }

    @Test
    fun `globalChatStart replays startCases`() {
        for (case in cases("startCases")) {
            val stops = case.getValue("stops").jsonArray.map { it.jsonObject }.map {
                FindInChat.Stop(
                    key = it.str("key")!!,
                    local = it.getValue("local").jsonPrimitive.int,
                    loaded = it.getValue("loaded").jsonPrimitive.boolean,
                    group = it.str("group"),
                    output = it.getValue("output").jsonPrimitive.boolean,
                )
            }
            val target = case.getValue("target").jsonObject
            val got = SearchList.globalChatStart(stops, SearchList.ChatTarget(target.str("key")!!, target.getValue("output").jsonPrimitive.boolean))
            assertEquals(case.name(), case["expected"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.int, got)
        }
    }
}
