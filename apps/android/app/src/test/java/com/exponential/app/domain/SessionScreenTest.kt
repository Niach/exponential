package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1732: THE session screen is fixture-locked ×4 — `session-screen.json`
 * `cases` (the WHOLE value), `menuCases` (the whole list), `kindCases`, `subtitleCases` and `statusCases` replay here (TS
 * `session-screen.test.ts`, iOS `SessionScreenTests`, desktop
 * `domain::session_screen`); the embedded copy, test ids, kinds, legacy tabs
 * and beside numbers are proved equal to the fixture's.
 */
class SessionScreenTest {

    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("session-screen.json")).jsonObject }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun cases(key: String): List<JsonObject> =
        (fixture[key] as JsonArray).map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    private fun nullable(value: String?): JsonElement = value?.let(::JsonPrimitive) ?: JsonNull

    private fun encode(screen: SessionScreen.Screen): JsonObject = buildJsonObject {
        put(
            "tabs",
            JsonArray(
                screen.tabs.map { tab ->
                    buildJsonObject {
                        put("id", tab.tab.wire)
                        put("label", tab.label)
                        put("count", tab.count?.let(::JsonPrimitive) ?: JsonNull)
                        put("disabled", tab.disabled)
                    }
                },
            ),
        )
        put("active", screen.active.wire)
        put("settings", screen.settings)
        put("open", nullable(screen.open?.wire))
        put("layout", screen.layout.wire)
        put("collapse", screen.collapse)
    }

    @Test
    fun `every menu case matches the fixture whole`() {
        for (case in cases("menuCases")) {
            val input = case.getValue("input").jsonObject
            fun flag(key: String) = input.getValue(key).jsonPrimitive.boolean
            val actual = SessionScreen.menu(
                kind = input.str("kind"),
                phone = flag("phone"),
                member = flag("member"),
                settings = flag("settings"),
                publish = flag("publish"),
                export = flag("export"),
                delete = flag("delete"),
                linkIssue = flag("linkIssue"),
            )
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                actual.map { it.wire },
            )
        }
    }

    @Test
    fun `the menu copy, icons and sends equal the fixture`() {
        val menu = fixture.getValue("copy").jsonObject.getValue("menu").jsonObject
        assertEquals(menu.str("publish"), SessionScreen.Copy.Menu.PUBLISH)
        assertEquals(menu.str("compact"), SessionScreen.Copy.Menu.COMPACT)
        assertEquals(menu.str("newConversation"), SessionScreen.Copy.Menu.NEW_CONVERSATION)
        assertEquals(menu.str("export"), SessionScreen.Copy.Menu.EXPORT)
        val settings = menu.getValue("settings").jsonObject
        assertEquals(settings.keys.toList(), SessionScreen.Copy.Menu.SETTINGS.keys.toList())
        for ((key, label) in SessionScreen.Copy.Menu.SETTINGS) assertEquals(settings.str(key), label)
        val delete = menu.getValue("delete").jsonObject
        assertEquals(delete.keys.toList(), SessionScreen.Copy.Menu.DELETE.keys.toList())
        for ((key, label) in SessionScreen.Copy.Menu.DELETE) assertEquals(delete.str(key), label)
        val icons = fixture.getValue("menuIcons").jsonObject
        assertEquals(icons.keys.toList(), SessionScreen.MENU_ICONS.keys.toList())
        for ((key, icon) in SessionScreen.MENU_ICONS) assertEquals(icons.str(key), icon)
        val sends = fixture.getValue("menuSends").jsonObject
        assertEquals(sends.keys.toList(), SessionScreen.MENU_SENDS.keys.toList())
        for ((key, line) in SessionScreen.MENU_SENDS) assertEquals(sends.str(key), line)
        assertEquals(
            SessionScreen.MenuEntry.entries.filter { it != SessionScreen.MenuEntry.Separator }.map { it.wire },
            icons.keys.toList(),
        )
    }

    @Test
    fun `every case matches the fixture whole`() {
        for (case in cases("cases")) {
            val input = case.getValue("input").jsonObject
            val counts = input.getValue("counts").jsonObject
            val actual = SessionScreen.screen(
                kind = input.str("kind"),
                wide = input.getValue("wide").jsonPrimitive.boolean,
                requested = input.str("requested"),
                open = input.str("open"),
                counts = SessionScreen.Counts(
                    issues = counts.getValue("issues").jsonPrimitive.int,
                    sessions = counts.getValue("sessions").jsonPrimitive.int,
                    results = counts.getValue("results").jsonPrimitive.int,
                ),
            )
            assertEquals(case.str("name"), case.getValue("expected"), encode(actual))
        }
    }

    @Test
    fun `every kind case matches the fixture`() {
        for (case in cases("kindCases")) {
            val input = case.getValue("input").jsonObject
            val actual = SessionScreen.kind(
                agentId = input.str("agentId"),
                appId = input.str("appId"),
                assistant = input.getValue("assistant").jsonPrimitive.boolean,
            )
            assertEquals(case.str("name"), case.str("expected"), actual.wire)
        }
    }

    @Test
    fun `every subtitle case matches the fixture`() {
        for (case in cases("subtitleCases")) {
            val input = case.getValue("input").jsonObject
            val actual = SessionScreen.subtitle(input.str("status"), input.str("device"), input.str("trigger"))
            assertEquals(case.str("name"), case.str("expected"), actual)
        }
    }

    @Test
    fun `every status case matches the fixture`() {
        for (case in cases("statusCases")) {
            val input = case.getValue("input").jsonArray.map { it.jsonObject }.map { session ->
                SessionScreen.StatusInput(
                    status = session.str("status") ?: "",
                    busy = session.getValue("busy").jsonPrimitive.boolean,
                    needsInput = session.getValue("needsInput").jsonPrimitive.boolean,
                    root = session.getValue("root").jsonPrimitive.boolean,
                )
            }
            assertEquals(case.str("name"), case.str("expected"), SessionScreen.status(input))
        }
    }

    @Test
    fun `the copy, test ids, kinds, legacy tabs and beside equal the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        val tabs = copy.getValue("tabs").jsonObject
        assertEquals(tabs.keys.toList(), SessionScreen.Copy.TABS.keys.toList())
        for ((key, label) in SessionScreen.Copy.TABS) assertEquals(tabs.str(key), label)
        assertEquals(SessionScreen.Tab.entries.map { it.wire }, tabs.keys.toList())
        assertEquals(copy.str("settings"), SessionScreen.Copy.SETTINGS)
        assertEquals(copy.str("back"), SessionScreen.Copy.BACK)
        assertEquals(copy.str("collapse"), SessionScreen.Copy.COLLAPSE)
        assertEquals(copy.str("separator"), SessionScreen.Copy.SEPARATOR)
        assertEquals(copy.str("linkIssue"), SessionScreen.Copy.LINK_ISSUE)
        val status = copy.getValue("status").jsonObject
        assertEquals(status.str("needsInput"), SessionScreen.Copy.Status.NEEDS_INPUT)
        assertEquals(status.str("working"), SessionScreen.Copy.Status.WORKING)
        assertEquals(status.str("queued"), SessionScreen.Copy.Status.QUEUED)
        assertEquals(status.str("done"), SessionScreen.Copy.Status.DONE)
        assertEquals(status.str("idle"), SessionScreen.Copy.Status.IDLE)

        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.str("tabs"), SessionScreen.TestIds.TABS)
        assertEquals(null, ids.str("gear"))
        assertEquals(ids.str("collapse"), SessionScreen.TestIds.COLLAPSE)
        assertEquals(ids.str("menu"), SessionScreen.TestIds.MENU)
        assertEquals(ids.str("beside"), SessionScreen.TestIds.BESIDE)
        assertEquals(ids.str("pane"), SessionScreen.TestIds.PANE)
        assertEquals(ids.str("issues"), SessionScreen.TestIds.ISSUES)
        assertEquals(ids.str("sessions"), SessionScreen.TestIds.SESSIONS)

        val kinds = fixture.getValue("kinds").jsonObject
        assertEquals(kinds.keys.toList(), SessionScreen.KINDS.keys.map { it.wire })
        for ((kind, spec) in SessionScreen.KINDS) {
            val json = kinds.getValue(kind.wire).jsonObject
            assertEquals(kind.wire, json.getValue("tabs").jsonArray.map { it.jsonPrimitive.content }, spec.tabs.map { it.wire })
            assertEquals(kind.wire, json.getValue("settings").jsonPrimitive.boolean, spec.settings)
            assertEquals(kind.wire, json.getValue("wideChatTab").jsonPrimitive.boolean, spec.wideChatTab)
            assertEquals(kind.wire, json.str("wideDefault"), spec.wideDefault.wire)
        }
        assertEquals(
            fixture.getValue("counted").jsonArray.map { it.jsonPrimitive.content },
            SessionScreen.COUNTED.map { it.wire },
        )
        val legacy = fixture.getValue("legacyTabs").jsonObject
        assertEquals(legacy.keys.toList(), SessionScreen.LEGACY_TABS.keys.toList())
        for ((key, value) in SessionScreen.LEGACY_TABS) assertEquals(legacy.str(key), value)

        val beside = fixture.getValue("beside").jsonObject
        assertEquals(beside.getValue("breakpoint").jsonPrimitive.int, SessionScreen.Beside.BREAKPOINT)
        assertEquals(beside.getValue("chat").jsonPrimitive.int, SessionScreen.Beside.CHAT)
        assertEquals(beside.getValue("pane").jsonPrimitive.int, SessionScreen.Beside.PANE)
        assertEquals(beside.getValue("chatMinWidth").jsonPrimitive.int, SessionScreen.Beside.CHAT_MIN_WIDTH)
    }
}
