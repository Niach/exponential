package com.exponential.app.data.push

import com.exponential.app.domain.contractFixtureJson
import kotlinx.serialization.json.Json
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

// #EXP-1641: answering from the notification, replayed off
// `notification-actions.json` (TS `notification-actions.test.ts`, same case names).
class NotificationActionsTest {
    private val fixture: JsonObject = Json.parseToJsonElement(contractFixtureJson("notification-actions.json")).jsonObject

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun cases(key: String): List<JsonObject> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    @Test
    fun `copy, data keys and limits are the fixture's`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("approve"), NotificationActions.APPROVE_LABEL)
        assertEquals(copy.str("requestChanges"), NotificationActions.REQUEST_CHANGES_LABEL)
        assertEquals(copy.str("requestChangesPlaceholder"), NotificationActions.REQUEST_CHANGES_PLACEHOLDER)
        assertEquals(copy.str("answer"), NotificationActions.ANSWER_LABEL)
        assertEquals(copy.str("answerPlaceholder"), NotificationActions.ANSWER_PLACEHOLDER)
        assertEquals(copy.str("reply"), NotificationActions.REPLY_LABEL)
        assertEquals(copy.str("replyPlaceholder"), NotificationActions.REPLY_PLACEHOLDER)
        assertEquals(copy.str("send"), NotificationActions.SEND_LABEL)
        assertEquals(copy.str("open"), NotificationActions.OPEN_LABEL)
        assertEquals(copy.str("failed"), NotificationActions.FAILED)
        val keys = fixture.getValue("dataKeys").jsonObject
        assertEquals(keys.str("messageId"), NotificationActions.KEY_MESSAGE_ID)
        assertEquals(keys.str("cardKind"), NotificationActions.KEY_CARD_KIND)
        assertEquals(keys.str("options"), NotificationActions.KEY_OPTIONS)
        assertEquals(keys.str("custom"), NotificationActions.KEY_CUSTOM)
        assertEquals(keys.str("planQuestions"), NotificationActions.KEY_PLAN_QUESTIONS)
        assertEquals(keys.str("sealed"), NotificationActions.KEY_SEALED)
        assertEquals(fixture.getValue("maxOptionButtons").jsonPrimitive.int, NotificationActions.MAX_OPTION_BUTTONS)
    }

    @Test
    fun `actionCases match the fixture`() {
        for (case in cases("actionCases")) {
            val input = case.getValue("input").jsonObject
            val data = input.getValue("data").jsonObject.mapValues { it.value.jsonPrimitive.content }
            val got = NotificationActions.notificationActions(
                data = data,
                keyHeld = input.getValue("keyHeld").jsonPrimitive.boolean,
                openedAskText = input.str("openedAskText"),
            )
            val expected = case.getValue("expected").jsonArray.map {
                val e = it.jsonObject
                val kind = if (e.str("kind") == "text") NotificationActions.Kind.Text else NotificationActions.Kind.Button
                NotificationActions.Action(e.str("id")!!, kind, e.str("title")!!)
            }
            assertEquals(case.str("name"), expected, got)
        }
    }

    @Test
    fun `optionCases match the fixture`() {
        for (case in cases("optionCases")) {
            val expected = case.getValue("expected").jsonObject
            val got = NotificationActions.askTextOptions(case.getValue("input").jsonPrimitive.content)
            assertEquals(case.str("name"), expected.getValue("options").jsonArray.map { it.jsonPrimitive.content }, got.options)
            assertEquals(case.str("name"), expected.getValue("custom").jsonPrimitive.boolean, got.custom)
        }
    }

    // #EXP-1641: a `sealed=1` push never leaves in clear, synced here or not.
    @Test
    fun `a sealed reply seals for the push's root, the session row not needed`() {
        val route = NotificationActions.route(NotificationActions.REPLY, sealed = true, rootId = "r", cardLocal = false)
        assertEquals(NotificationActions.Route.SendSealed("r"), route)
    }

    @Test
    fun `a sealed reply without a root is refused`() {
        assertEquals(NotificationActions.Route.Refuse, NotificationActions.route(NotificationActions.REPLY, true, null, false))
        assertEquals(NotificationActions.Route.Refuse, NotificationActions.route(NotificationActions.REPLY, true, "", false))
    }

    @Test
    fun `a plain reply is the composer's send`() {
        assertEquals(NotificationActions.Route.Send, NotificationActions.route(NotificationActions.REPLY, false, null, false))
    }

    @Test
    fun `a sealed card not synced here is read first, then answered`() {
        for (id in listOf(NotificationActions.APPROVE, NotificationActions.REQUEST_CHANGES, NotificationActions.ANSWER, "option:1")) {
            assertEquals(id, NotificationActions.Route.FetchThenAnswer, NotificationActions.route(id, sealed = true, rootId = "r", cardLocal = false))
            assertEquals(id, NotificationActions.Route.Answer, NotificationActions.route(id, sealed = true, rootId = "r", cardLocal = true))
            assertEquals(id, NotificationActions.Route.Answer, NotificationActions.route(id, sealed = false, rootId = null, cardLocal = false))
        }
    }

    @Test
    fun `open and unknown ids run nothing`() {
        assertEquals(NotificationActions.Route.Refuse, NotificationActions.route(NotificationActions.OPEN, false, null, true))
        assertEquals(NotificationActions.Route.Refuse, NotificationActions.route("inbox", true, "r", true))
    }
}
