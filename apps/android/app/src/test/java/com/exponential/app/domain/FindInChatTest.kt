package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
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
 * #EDE-130: find in chat is fixture-locked ×4: every case group of
 * `find-in-chat.json` replays here (TS `find-in-chat.test.ts`, desktop
 * `domain::find_in_chat`, iOS `FindInChatTests`); the embedded copy, test
 * ids, constants and shortcut are proved equal to the fixture's.
 */
class FindInChatTest {

    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("find-in-chat.json")).jsonObject }

    private fun cases(key: String): List<JsonObject> =
        (fixture[key] as JsonArray).map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.name(): String = str("name").orEmpty()

    private fun JsonElement?.intOrNull(): Int? = this?.takeUnless { it is JsonNull }?.jsonPrimitive?.int

    private fun stop(o: JsonObject) = FindInChat.Stop(
        key = o.str("key")!!,
        local = o.getValue("local").jsonPrimitive.int,
        loaded = o.getValue("loaded").jsonPrimitive.boolean,
        group = o.str("group"),
        output = o.getValue("output").jsonPrimitive.boolean,
    )

    private fun stops(a: JsonElement): List<FindInChat.Stop> = a.jsonArray.map { stop(it.jsonObject) }

    @Test
    fun `constants copy test ids and shortcut match the fixture`() {
        assertEquals(fixture.getValue("queryMaxChars").jsonPrimitive.int, FindInChat.QUERY_MAX_CHARS)
        assertEquals(fixture.getValue("hitsMax").jsonPrimitive.int, FindInChat.HITS_MAX)
        assertEquals(fixture.getValue("debounceMs").jsonPrimitive.int.toLong(), FindInChat.DEBOUNCE_MS)
        val copy = fixture.getValue("copy").jsonObject
        val ours = mapOf(
            "menu" to FindInChat.Copy.MENU,
            "placeholder" to FindInChat.Copy.PLACEHOLDER,
            "counter" to FindInChat.Copy.COUNTER,
            "counterCompact" to FindInChat.Copy.COUNTER_COMPACT,
            "noResults" to FindInChat.Copy.NO_RESULTS,
            "searchingOlder" to FindInChat.Copy.SEARCHING_OLDER,
            "searchingDevice" to FindInChat.Copy.SEARCHING_DEVICE,
            "deviceOffline" to FindInChat.Copy.DEVICE_OFFLINE,
            "outputHintOne" to FindInChat.Copy.OUTPUT_HINT_ONE,
            "outputHintMany" to FindInChat.Copy.OUTPUT_HINT_MANY,
            "previous" to FindInChat.Copy.PREVIOUS,
            "next" to FindInChat.Copy.NEXT,
            "close" to FindInChat.Copy.CLOSE,
        )
        assertEquals(copy.keys, ours.keys)
        for ((key, value) in ours) assertEquals(key, copy.str(key), value)
        val ids = fixture.getValue("testIds").jsonObject
        val ourIds = mapOf(
            "bar" to FindInChat.TestIds.BAR,
            "input" to FindInChat.TestIds.INPUT,
            "counter" to FindInChat.TestIds.COUNTER,
            "previous" to FindInChat.TestIds.PREVIOUS,
            "next" to FindInChat.TestIds.NEXT,
            "close" to FindInChat.TestIds.CLOSE,
            "status" to FindInChat.TestIds.STATUS,
            "outputHint" to FindInChat.TestIds.OUTPUT_HINT,
        )
        assertEquals(ids.keys, ourIds.keys)
        for ((key, value) in ourIds) assertEquals(key, ids.str(key), value)
        val shortcut = fixture.getValue("shortcut").jsonObject
        assertEquals(shortcut.str("mac"), FindInChat.shortcut(apple = true))
        assertEquals(shortcut.str("other"), FindInChat.shortcut(apple = false))
    }

    @Test
    fun `every menu case`() {
        for (case in cases("menuCases")) {
            val entries = case.getValue("entries").jsonArray.map { it.jsonPrimitive.content }
            val expected = case.getValue("expected").jsonArray.map { it.jsonPrimitive.content }
            assertEquals(case.name(), expected, FindInChat.menu(case.str("kind"), entries))
        }
    }

    @Test
    fun `every query case`() {
        for (case in cases("queryCases")) {
            assertEquals(case.name(), case.str("query"), FindInChat.query(case.str("raw")!!))
        }
    }

    @Test
    fun `every match case`() {
        for (case in cases("matchCases")) {
            val rows = case.getValue("rows").jsonArray.map { r ->
                val o = r.jsonObject
                FindInChat.Row(
                    key = o.str("key")!!,
                    count = o.getValue("count").jsonPrimitive.int,
                    group = o.str("group"),
                    outputHit = o["outputHit"]?.jsonPrimitive?.boolean == true,
                )
            }
            val older = case.getValue("older").jsonArray.map { h ->
                val o = h.jsonObject
                FindInChat.OlderHit(o.str("key")!!, o["output"]?.jsonPrimitive?.boolean == true)
            }
            assertEquals(case.name(), stops(case.getValue("expected")), FindInChat.stops(rows, older))
        }
    }

    @Test
    fun `every older case`() {
        fun JsonElement?.longOrNull(): Long? = this?.takeUnless { it is JsonNull }?.jsonPrimitive?.content?.toLong()
        for (case in cases("olderCases")) {
            val store = case.getValue("store").jsonArray.map { h ->
                val o = h.jsonObject
                FindInChat.StoreHit(o.str("sessionId")!!, o["seq"].longOrNull(), o.str("messageId"), o.str("kind")!!)
            }
            val device = case.getValue("device").jsonArray.map { h ->
                val o = h.jsonObject
                FindInChat.DeviceHit(o["seq"].longOrNull()!!, o.str("kind")!!)
            }
            val synced = case["synced"]?.takeUnless { it is JsonNull }?.jsonObject?.let { o ->
                FindInChat.Synced(
                    loaded = o.getValue("loaded").jsonArray.map { it.jsonPrimitive.content },
                    drawable = o.getValue("drawable").jsonArray.map { it.jsonPrimitive.content },
                    more = o.getValue("more").jsonPrimitive.boolean,
                )
            }
            val expected = case.getValue("expected").jsonArray.map { h ->
                val o = h.jsonObject
                FindInChat.OlderHit(o.str("key")!!, o.getValue("output").jsonPrimitive.boolean)
            }
            val got = FindInChat.older(case.str("sessionId")!!, case["lowestSeq"].longOrNull(), store, device, synced)
            assertEquals(case.name(), expected, got)
        }
    }

    @Test
    fun `every counter case`() {
        for (case in cases("counterCases")) {
            val actual = FindInChat.counter(
                current = case["current"].intOrNull(),
                total = case.getValue("total").jsonPrimitive.int,
                done = case.getValue("done").jsonPrimitive.boolean,
                compact = case.getValue("compact").jsonPrimitive.boolean,
            )
            assertEquals(case.name(), case.str("expected"), actual)
        }
    }

    @Test
    fun `every step case`() {
        for (case in cases("stepCases")) {
            val step = FindInChat.Step.entries.first { it.wire == case.str("step") }
            val actual = FindInChat.step(case["current"].intOrNull(), case.getValue("total").jsonPrimitive.int, step)
            assertEquals(case.name(), case["expected"].intOrNull(), actual)
        }
    }

    @Test
    fun `every result case`() {
        for (case in cases("resultCases")) {
            val actual = FindInChat.keep(stops(case.getValue("before")), case["current"].intOrNull(), stops(case.getValue("after")))
            assertEquals(case.name(), case["expected"].intOrNull(), actual)
        }
    }

    @Test
    fun `every land case`() {
        for (case in cases("landCases")) {
            val land = FindInChat.land(stop(case.getValue("match").jsonObject))
            val expected = case.getValue("expected").jsonObject
            assertEquals(case.name(), expected.str("openGroup"), land.openGroup)
            assertEquals(case.name(), expected.getValue("pageIn").jsonPrimitive.boolean, land.pageIn)
            assertEquals(case.name(), expected.str("scroll"), land.scroll)
            assertEquals(case.name(), expected.getValue("ring").jsonPrimitive.boolean, land.ring)
            assertEquals(case.name(), expected["current"].intOrNull(), land.current)
        }
    }

    @Test
    fun `every status case`() {
        fun phase(wire: String?) = FindInChat.Phase.entries.first { it.wire == wire }
        for (case in cases("statusCases")) {
            val input = case.getValue("input").jsonObject
            val actual = FindInChat.status(phase(input.str("store")), phase(input.str("device")), input.str("deviceName")!!)
            assertEquals(case.name(), case.str("expected"), actual)
        }
    }

    @Test
    fun `every key case`() {
        for (case in cases("keyCases")) {
            val input = case.getValue("input").jsonObject
            fun flag(key: String) = input.getValue(key).jsonPrimitive.boolean
            val actual = FindInChat.key(input.str("key")!!, flag("mod"), flag("shift"), flag("inChat"), flag("inField"))
            assertEquals(case.name(), case.str("expected"), actual?.wire)
        }
    }

    @Test
    fun `output hint and keys`() {
        assertEquals("1 match in output", FindInChat.outputHint(1))
        assertEquals("3 matches in output", FindInChat.outputHint(3))
        assertEquals("e:s1:4", FindInChat.eventKey("s1", 4))
        assertEquals("m:a1", FindInChat.messageKey("a1"))
        assertEquals(4L, FindInChat.eventSeq("e:s1:4"))
        assertEquals(4L, FindInChat.eventSeq("e:s1:4#9"))
        assertEquals(null, FindInChat.eventSeq("m:a1"))
    }
}
