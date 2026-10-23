package com.exponential.app.ui.styleguide

import com.exponential.app.domain.Chats
import com.exponential.app.domain.contractFixtureJson
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Thread 58cb32df: the chat-list styleguide specimen draws THE FIXTURE ×4 —
 * chats.json `listCases[0].expected` and `bubbleCases[0].expected`, what the
 * web entry and the desktop twin render. The app has no fixture at runtime,
 * so [ChatListSpecimen] spells it and this proves it equal.
 */
class ChatSpecimenFixtureTest {

    private val fixture by lazy { Json.parseToJsonElement(contractFixtureJson("chats.json")).jsonObject }

    private fun nullable(value: String?): JsonElement = value?.let(::JsonPrimitive) ?: JsonNull

    @Test
    fun `the chat-list specimen is listCases 0 expected`() {
        val expected = fixture.getValue("listCases").jsonArray[0].jsonObject.getValue("expected")
        val list = ChatListSpecimen.LIST
        val actual = buildJsonObject {
            put(
                "rows",
                JsonArray(
                    list.rows.map { row ->
                        buildJsonObject {
                            put("sessionId", row.sessionId)
                            put("teamId", row.teamId)
                            put("title", row.title)
                            put("deviceId", nullable(row.deviceId))
                            put("group", row.group.wire)
                            put("lead", row.lead.wire)
                            put("preview", buildJsonObject { put("kind", row.preview.kind); put("text", row.preview.text) })
                            put("activityAt", row.activityAt)
                            put("time", row.time)
                            put("unreadCount", row.unreadCount)
                            put("needsYou", row.needsYou)
                            put("busy", row.busy)
                            put("word", nullable(row.word?.wire))
                        }
                    },
                ),
            )
            put(
                "counts",
                buildJsonObject {
                    put("needs_you", list.counts.needsYou)
                    put("running", list.counts.running)
                    put("unread", list.counts.unread)
                },
            )
        }
        assertEquals(expected, actual)
    }

    @Test
    fun `the bubbles specimen is bubbleCases 0 expected`() {
        val expected = fixture.getValue("bubbleCases").jsonArray[0].jsonObject.getValue("expected")
        val actual = JsonArray(
            ChatListSpecimen.BUBBLES.map { b ->
                buildJsonObject {
                    put("agentId", b.agent.agentId)
                    put("teamId", b.agent.teamId)
                    put("name", b.agent.name)
                    put("icon", b.agent.icon)
                    put("color", b.agent.color)
                    put("busy", b.agent.busy)
                    put("needsYou", b.agent.needsYou)
                    put("ring", nullable(b.ring?.wire))
                }
            },
        )
        assertEquals(expected, actual)
        // The chips read the counts through chatFilterLabel.
        assertEquals("Needs you 1", Chats.chatFilterLabel(Chats.ChatFilter.NeedsYou, ChatListSpecimen.LIST.counts))
    }
}
