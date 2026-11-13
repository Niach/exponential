package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #FEED-237: [MessageDelivery] replays `fixtures/message-delivery.json`
 * (TS `message-delivery.test.ts`, desktop `domain::message_delivery`, iOS
 * `MessageDelivery.swift`): every `stateCases`, `outboxCases` and
 * `actionCases` entry, plus the copy, marks and test ids.
 */
class MessageDeliveryTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("message-delivery.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    @Test
    fun `copy marks and test ids match the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        val status = copy.getValue("status").jsonObject
        for (state in MessageDelivery.State.entries) {
            assertEquals(state.wire, status.str(state.wire), state.label)
        }
        val menu = copy.getValue("menu").jsonObject
        for (action in MessageDelivery.Action.entries) {
            assertEquals(action.wire, menu.str(action.wire), action.label)
        }
        val marks = fixture.getValue("marks").jsonObject
        assertEquals(marks.keys, MessageDelivery.State.entries.map { it.wire }.toSet())
        for (state in MessageDelivery.State.entries) {
            val mark = marks.getValue(state.wire).jsonObject
            assertEquals(state.wire, mark.str("icon"), state.icon)
            assertEquals(state.wire, mark.str("tone"), state.tone.wire)
        }
        assertEquals(fixture.getValue("markSize").jsonPrimitive.long.toInt(), MessageDelivery.MARK_SIZE)
        assertEquals(fixture.getValue("matchWindowMs").jsonPrimitive.long, MessageDelivery.MATCH_WINDOW_MS)
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.str("status"), MessageDelivery.TestIds.STATUS)
        assertEquals(ids.str("menu"), MessageDelivery.TestIds.MENU)
    }

    @Test
    fun `every state case matches the fixture`() {
        val cases = fixture.getValue("stateCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = MessageDelivery.deliveryState(
                local = MessageDelivery.Local.of(input.str("local")),
                stored = input.getValue("stored").jsonPrimitive.boolean,
                deliveredAt = input.str("deliveredAt"),
                consumedAt = input.str("consumedAt"),
                echoed = input.getValue("echoed").jsonPrimitive.boolean,
            )
            assertEquals(case.str("name"), case.str("expected"), actual.wire)
        }
    }

    @Test
    fun `every outbox case matches the fixture`() {
        val cases = fixture.getValue("outboxCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val outbox = input.getValue("outbox").jsonArray.map { it.jsonObject }.map { e ->
                MessageDelivery.Entry(
                    clientId = e.str("clientId")!!,
                    text = e.str("text")!!,
                    createdAt = e.str("createdAt")!!,
                    state = MessageDelivery.Local.of(e.str("state"))!!,
                    messageId = e.str("messageId"),
                )
            }
            val rows = input.getValue("rows").jsonArray.map { it.jsonObject }.map { r ->
                MessageDelivery.Row(
                    id = r.str("id")!!,
                    authorKind = r.str("authorKind")!!,
                    authorId = r.str("authorId"),
                    body = r.str("body")!!,
                    createdAt = r.str("createdAt")!!,
                )
            }
            val expected = case.getValue("expected").jsonObject
            val actual = MessageDelivery.outboxMerge(input.str("viewerId"), outbox, rows)
            assertEquals(
                case.str("name"),
                MessageDelivery.Merge(
                    drop = expected.getValue("drop").jsonArray.map { it.jsonPrimitive.content },
                    keep = expected.getValue("keep").jsonArray.map { it.jsonPrimitive.content },
                ),
                actual,
            )
        }
    }

    @Test
    fun `every action case matches the fixture`() {
        val cases = fixture.getValue("actionCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = MessageDelivery.bubbleActions(
                state = MessageDelivery.State.of(input.str("state")!!),
                stored = input.getValue("stored").jsonPrimitive.boolean,
            )
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                actual.map { it.wire },
            )
        }
    }

    @Test
    fun `the tail is the unconsumed own rows then the kept entries`() {
        val outbox = listOf(
            MessageDelivery.Entry("c1", "ship it", "2026-11-13T10:00:00.000Z", MessageDelivery.Local.Sending, "m1"),
            MessageDelivery.Entry("c2", "and the docs", "2026-11-13T10:00:02.000Z", MessageDelivery.Local.Failed, null),
            MessageDelivery.Entry("c3", "later", "2026-11-13T10:00:03.000Z", MessageDelivery.Local.Sending, "m9"),
        )
        val rows = listOf(
            MessageDelivery.Row("m0", "user", "u1", "old", "2026-11-13T09:00:00.000Z", consumedAt = "2026-11-13T09:00:05.000Z"),
            MessageDelivery.Row("m1", "user", "u1", "ship it", "2026-11-13T10:00:00.400Z", deliveredAt = "2026-11-13T10:00:01.000Z"),
            MessageDelivery.Row("m2", "user", "u2", "theirs", "2026-11-13T10:00:00.500Z"),
        )
        val tail = MessageDelivery.tail("u1", outbox, rows)
        assertEquals(listOf("c1"), tail.drop)
        assertEquals(listOf("own-row-m1", "own-entry-c2", "own-entry-c3"), tail.bubbles.map { it.key })
        assertEquals(
            listOf(MessageDelivery.State.Sent, MessageDelivery.State.Failed, MessageDelivery.State.Sending),
            tail.bubbles.map { it.state },
        )
        assertEquals(
            listOf(listOf(MessageDelivery.Action.Cancel), listOf(MessageDelivery.Action.Retry, MessageDelivery.Action.Delete), listOf(MessageDelivery.Action.Cancel)),
            tail.bubbles.map { it.actions },
        )
    }
}
