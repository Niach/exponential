package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// #EXP-1638 / #EXP-1830: the app icon's long-press menu, replayed off
// `app-shortcuts.json` (TS `app-shortcuts.test.ts`, same case names).
class AppShortcutsTest {
    private val fixture: JsonObject = Json.parseToJsonElement(contractFixtureJson("app-shortcuts.json")).jsonObject

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun cases(key: String): List<JsonObject> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    private fun input(json: JsonObject): AppShortcuts.Input = AppShortcuts.Input(
        assistant = json["assistant"]?.takeUnless { it is JsonNull }?.jsonObject?.let {
            AppShortcuts.Assistant(it.str("id")!!, it.str("name")!!)
        },
        agents = json.getValue("agents").jsonArray.map {
            val a = it.jsonObject
            AppShortcuts.Agent(a.str("id")!!, a.str("name")!!, a.str("icon"), a.str("color"))
        },
        pins = json.getValue("pins").jsonArray.map { it.jsonPrimitive.content },
        chats = json.getValue("chats").jsonArray.map {
            val c = it.jsonObject
            ShareTargets.Chat(c.str("id")!!, c.str("title")!!, c.str("updatedAt")!!, c.str("agentId"), c["sealed"]?.jsonPrimitive?.booleanOrNull == true)
        },
    )

    private fun expected(case: JsonObject): List<AppShortcuts.Shortcut> =
        case.getValue("expected").jsonArray.map {
            val e = it.jsonObject
            AppShortcuts.Shortcut(e.str("id")!!, e.str("title")!!, e.str("icon")!!, e.str("color"))
        }

    @Test
    fun `copy, icons, ids and limits are the fixture's`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("newIssue"), AppShortcuts.NEW_ISSUE_LABEL)
        assertEquals(copy.str("newChat"), AppShortcuts.NEW_CHAT_LABEL)
        assertEquals(copy.str("search"), AppShortcuts.SEARCH_LABEL)
        val icons = fixture.getValue("icons").jsonObject
        assertEquals(icons.str("newIssue"), AppShortcuts.NEW_ISSUE_ICON)
        assertEquals(icons.str("newChat"), AppShortcuts.NEW_CHAT_ICON)
        assertEquals(icons.str("search"), AppShortcuts.SEARCH_ICON)
        assertEquals(icons.str("chat"), AppShortcuts.CHAT_ICON)
        assertEquals(icons.str("assistant"), AppShortcuts.ASSISTANT_ICON)
        assertEquals(icons.str("agent"), AppShortcuts.AGENT_ICON)
        assertEquals(fixture.getValue("statics").jsonArray.map { it.jsonPrimitive.content }, AppShortcuts.STATICS)
        assertEquals(fixture.str("recentChatPrefix"), AppShortcuts.RECENT_CHAT_PREFIX)
        assertEquals(fixture.str("agentPrefix"), AppShortcuts.AGENT_PREFIX)
        assertEquals(fixture.getValue("maxEntries").jsonPrimitive.int, AppShortcuts.MAX_ENTRIES)
    }

    @Test
    fun `listCases match the fixture`() {
        for (case in cases("listCases")) {
            val got = AppShortcuts.appShortcuts(input(case.getValue("input").jsonObject))
            assertEquals(case.str("name"), expected(case), got)
        }
    }

    @Test
    fun `chatEntryCases match the fixture`() {
        for (case in cases("chatEntryCases")) {
            val got = AppShortcuts.chatEntries(input(case.getValue("input").jsonObject))
            assertEquals(case.str("name"), expected(case), got)
        }
    }

    @Test
    fun `parseCases match the fixture`() {
        for (case in cases("parseCases")) {
            val got = AppShortcuts.parseAppShortcut(case.getValue("input").jsonPrimitive.content)
            val expected = case["expected"]?.takeUnless { it is JsonNull }?.jsonObject
            val kind = when (got) {
                null -> null
                AppShortcuts.Target.NewIssue -> "new-issue"
                AppShortcuts.Target.NewChat -> "new-chat"
                AppShortcuts.Target.Search -> "search"
                is AppShortcuts.Target.Chat -> "chat"
                is AppShortcuts.Target.Agent -> "agent"
            }
            val id = when (got) {
                is AppShortcuts.Target.Chat -> got.id
                is AppShortcuts.Target.Agent -> got.id
                else -> null
            }
            assertEquals(case.str("name"), expected?.str("kind"), kind)
            assertEquals(case.str("name"), expected?.str("id"), id)
        }
    }

    @Test
    fun `launcherCases match the fixture`() {
        for (case in cases("launcherCases")) {
            val got = AppShortcuts.launcherShortcutId(case.getValue("input").jsonPrimitive.content)
            assertEquals(case.str("name"), case.str("expected"), got)
        }
    }
}
