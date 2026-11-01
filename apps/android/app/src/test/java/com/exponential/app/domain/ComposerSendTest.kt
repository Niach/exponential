package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1537: [ComposerSend] replays `fixtures/composer-send.json` and compares
 * the WHOLE result, like the TS reference's own test, desktop
 * `domain::composer_send` and iOS `ComposerSend.swift`.
 */
class ComposerSendTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("composer-send.json")).jsonObject
    }

    private fun JsonObject.content(key: String): ComposerSend.Content {
        val obj = getValue(key).jsonObject
        return ComposerSend.Content(
            text = obj.getValue("text").jsonPrimitive.content,
            attachments = obj.getValue("attachments").jsonArray.map { it.jsonPrimitive.content },
        )
    }

    private fun outcome(wire: String): ComposerSend.Outcome = when (wire) {
        "sent" -> ComposerSend.Outcome.Sent
        "failed" -> ComposerSend.Outcome.Failed
        else -> error("unknown outcome $wire")
    }

    private val cases: List<JsonObject>
        get() = fixture.getValue("cases").jsonArray.map { it.jsonObject }

    @Test
    fun `every case leaves the whole expected composer`() {
        assertTrue(cases.size >= 10)
        for (case in cases) {
            val name = case.getValue("name").jsonPrimitive.content
            val input = case.getValue("input").jsonObject
            val actual = ComposerSend.afterSend(
                outcome = outcome(input.getValue("outcome").jsonPrimitive.content),
                sent = input.content("sent"),
                current = input.content("current"),
            )
            assertEquals(name, case.content("expected"), actual)
        }
    }

    @Test
    fun `the text half matches every attachment-free case`() {
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            if (input.getValue("outcome").jsonPrimitive.content != "sent") continue
            val sent = input.content("sent")
            val current = input.content("current")
            if (sent.attachments.isNotEmpty() || current.attachments.isNotEmpty()) continue
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                case.content("expected").text,
                ComposerSend.textAfterSend(sent.text, current.text),
            )
        }
    }
}
