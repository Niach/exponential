package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * [WaitingCards] replays `fixtures/waiting-cards.json` (`stateCases`,
 * `chipCases`, `targetCases`; `dismissCases` are the server's), comparing
 * WHOLE values.
 */
class WaitingCardsTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("waiting-cards.json")).jsonObject
    }

    private fun cases(key: String): List<JsonObject> = fixture.getValue(key).jsonArray.map { it.jsonObject }

    private fun JsonElement?.strOrNull(): String? =
        (this as? JsonPrimitive)?.takeIf { it.isString }?.content

    private fun map(key: String): Map<String, String> =
        fixture.getValue(key).jsonObject.mapValues { it.value.jsonPrimitive.content }

    @Test
    fun `constants match the fixture`() {
        assertEquals(
            mapOf(
                "dismiss" to WaitingCards.Copy.DISMISS, "dismissed" to WaitingCards.Copy.DISMISSED,
                "waiting" to WaitingCards.Copy.WAITING, "waitingCount" to WaitingCards.Copy.WAITING_COUNT,
            ),
            map("copy"),
        )
        assertEquals(
            mapOf(
                "plan" to WaitingCards.Icons.PLAN, "question" to WaitingCards.Icons.QUESTION,
                "dismiss" to WaitingCards.Icons.DISMISS, "waiting" to WaitingCards.Icons.WAITING,
            ),
            map("icons"),
        )
        assertEquals(
            mapOf(
                "labelMax" to WaitingCards.Constants.LABEL_MAX.toString(),
                "ellipsis" to WaitingCards.Constants.ELLIPSIS,
                "highlightMs" to WaitingCards.Constants.HIGHLIGHT_MS.toString(),
            ),
            map("constants"),
        )
        assertEquals(
            mapOf(
                "dismiss" to WaitingCards.TestIds.DISMISS, "dismissed" to WaitingCards.TestIds.DISMISSED,
                "chip" to WaitingCards.TestIds.CHIP, "button" to WaitingCards.TestIds.BUTTON,
                "menu" to WaitingCards.TestIds.MENU,
            ),
            map("testIds"),
        )
        assertEquals("3 waiting", WaitingCards.waitingCountLabel(3))
    }

    @Test
    fun `every state case matches the fixture`() {
        for (case in cases("stateCases")) {
            val name = case.getValue("name").jsonPrimitive.content
            val card = case["card"] as? JsonObject
            val actual = WaitingCards.cardState(card, case["answeredAt"].strOrNull())
            assertEquals(name, case["expected"].strOrNull(), actual?.wire)
        }
    }

    private fun kind(wire: String) = AskDock.Kind.entries.first { it.wire == wire }

    @Test
    fun `every chip case matches the fixture`() {
        for (case in cases("chipCases")) {
            val name = case.getValue("name").jsonPrimitive.content
            val cards = case.getValue("cards").jsonArray.map { it.jsonObject }.map {
                WaitingCards.Input(
                    id = it.getValue("id").jsonPrimitive.content,
                    kind = kind(it.getValue("kind").jsonPrimitive.content),
                    text = it.getValue("text").jsonPrimitive.content,
                )
            }
            val result = WaitingCards.waitingChips(cards, case.getValue("phone").jsonPrimitive.boolean)
            val actual = buildJsonObject {
                put("layout", result.layout.wire)
                put("count", result.count)
                put(
                    "chips",
                    JsonArray(
                        result.chips.map { chip ->
                            buildJsonObject {
                                put("id", chip.id)
                                put("kind", chip.kind.wire)
                                put("icon", chip.icon)
                                put("label", chip.label)
                            }
                        },
                    ),
                )
            }
            assertEquals(name, case.getValue("expected"), actual)
        }
    }

    @Test
    fun `every target case matches the fixture`() {
        val cases = cases("targetCases")
        assertEquals(true, cases.isNotEmpty())
        for (case in cases) {
            val name = case.getValue("name").jsonPrimitive.content
            val card = case.getValue("card").jsonObject
            val target = WaitingCards.waitingTarget(
                kind = kind(card.getValue("kind").jsonPrimitive.content),
                sessionId = card.getValue("sessionId").jsonPrimitive.content,
                rootId = case.getValue("rootId").jsonPrimitive.content,
            )
            val actual = buildJsonObject {
                put("open", target.open.wire)
                put("sessionId", target.sessionId)
            }
            assertEquals(name, case.getValue("expected"), actual)
        }
    }

    @Test
    fun `chat pending asks carry the asking session`() {
        val rows = listOf(
            SessionTree.SessionChatRow.Question(
                id = "q", sessionId = "c1", question = "Which?", options = emptyList(), custom = true,
                answer = null, answered = false, dismissed = false,
            ),
        )
        val inputs = WaitingCards.inputs(AskDock.chatPendingAsks(rows) { null })
        assertEquals(listOf(WaitingCards.Input("q", AskDock.Kind.Question, "Which?", "c1")), inputs)
        assertEquals("c1", WaitingCards.waitingChips(inputs, phone = true).chips.single().sessionId)
    }

    @Test
    fun `a dismissed question row is no chat pending ask`() {
        val rows = listOf(
            SessionTree.SessionChatRow.Question(
                id = "q", sessionId = "r", question = "Which?", options = emptyList(), custom = true,
                answer = null, answered = true, dismissed = true,
            ),
            SessionTree.SessionChatRow.Plan(id = "p", sessionId = "r", plan = "1.", decision = AskDock.DECISION_DISMISSED),
        )
        assertEquals(emptyList<AskDock.Pending>(), AskDock.chatPendingAsks(rows) { null })
    }
}
