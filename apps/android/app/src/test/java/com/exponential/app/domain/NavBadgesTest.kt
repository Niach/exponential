package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** #FEED-232: every case list of `nav-badges.json` replays here (TS
 *  `nav-badges.test.ts`, iOS `NavBadgesTests`, desktop `domain::nav_badges`). */
class NavBadgesTest {

    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("nav-badges.json")).jsonObject }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.bool(key: String): Boolean = getValue(key).jsonPrimitive.boolean

    private fun JsonObject.int(key: String): Int = getValue(key).jsonPrimitive.int

    private fun JsonObject.list(key: String): List<JsonObject> =
        (this[key] as? JsonArray).orEmpty().map { it.jsonObject }

    private fun cases(key: String): List<JsonObject> = fixture.list(key).also { assertTrue(key, it.isNotEmpty()) }

    private fun tone(wire: String) = when (wire) {
        "attention" -> NavBadges.Tone.Attention
        "muted" -> NavBadges.Tone.Muted
        else -> error("unknown tone $wire")
    }

    private fun expected(case: JsonObject): NavBadges.Badge {
        val e = case.getValue("expected").jsonObject
        return NavBadges.Badge(e.int("count"), tone(e.str("tone")!!))
    }

    @Test
    fun `the cap matches the fixture`() {
        assertEquals(fixture.int("max"), NavBadges.MAX)
    }

    @Test
    fun `every label case matches the fixture`() {
        for (case in cases("labelCases")) {
            assertEquals(case.str("name"), case.str("expected"), NavBadges.countBadgeLabel(case.int("count")))
        }
    }

    @Test
    fun `every chats case matches the fixture`() {
        for (case in cases("chatsCases")) {
            val input = case.getValue("input").jsonObject
            val groups = input.list("groups").map { group ->
                SessionTree.ChatsListGroup(
                    // The key never matters to the count (the fixture says `working`).
                    key = SessionTree.ChatsGroupKey.entries.firstOrNull { it.wire == group.str("key") }
                        ?: SessionTree.ChatsGroupKey.Running,
                    title = group.str("title")!!,
                    rows = group.list("rows").map {
                        SessionTree.ChatsListRow(
                            sessionId = it.str("sessionId")!!,
                            teamId = it.str("teamId")!!,
                            title = it.str("title")!!,
                            busy = it.bool("busy"),
                            needsYou = it.bool("needsYou"),
                            activityAt = it.str("activityAt")!!,
                            deviceId = it.str("deviceId"),
                            depth = it.int("depth"),
                            parentId = it.str("parentId"),
                            rootId = it.str("rootId")!!,
                            unread = it.bool("unread"),
                        )
                    },
                )
            }
            val reads = input.list("reads").map {
                SessionTree.SessionRead(it.str("sessionId")!!, it.str("readAt")!!, it.str("archivedAt"))
            }
            assertEquals(case.str("name"), expected(case), NavBadges.chatsBadge(groups, reads))
        }
    }

    @Test
    fun `every row case matches the fixture`() {
        for (case in cases("rowCases")) {
            val input = case.getValue("input").jsonObject
            assertEquals(
                case.str("name"),
                expected(case),
                NavBadges.rowBadge(input.int("unreadCount"), input.bool("needsYou")),
            )
        }
    }

    @Test
    fun `every app icon case matches the fixture`() {
        for (case in cases("appIconCases")) {
            assertEquals(
                case.str("name"),
                case.int("expected"),
                NavBadges.appIconBadge(case.list("teams").map { it.int("needsYou") }),
            )
        }
    }
}
