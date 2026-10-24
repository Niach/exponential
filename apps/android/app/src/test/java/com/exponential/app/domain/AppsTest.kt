package com.exponential.app.domain

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

// VAPP-110: replays `packages/domain-contract/fixtures/apps.json` (web,
// desktop and iOS replay the same cases) and locks the copy + caps.
class AppsTest {
    private fun read(relative: String): JsonObject {
        val candidates = listOf("../../../$relative", "../../$relative", relative)
        val file = candidates.map(::File).firstOrNull { it.isFile } ?: error("$relative not found from ${File(".").absolutePath}")
        return Json.parseToJsonElement(file.readText()).jsonObject
    }

    private val fixture = read("packages/domain-contract/fixtures/apps.json")
    private val contract = read("packages/domain-contract/contract.json")

    private data class Row(override val slug: String, override val name: String) : Apps.ListLike

    @Test
    fun `slugify replays the fixture`() {
        for (case in fixture["slugifyCases"]!!.jsonArray.map { it.jsonObject }) {
            val taken = case["taken"]?.jsonArray?.map { it.jsonPrimitive.content }.orEmpty()
            assertEquals(case.toString(), case["slug"]!!.jsonPrimitive.content, Apps.slugify(case["name"]!!.jsonPrimitive.content, taken))
        }
    }

    @Test
    fun `versionLabel replays the fixture`() {
        for (case in fixture["versionLabelCases"]!!.jsonArray.map { it.jsonObject }) {
            assertEquals(case["label"]!!.jsonPrimitive.content, Apps.versionLabel(case["headVersion"]!!.jsonPrimitive.int))
        }
    }

    @Test
    fun `sortApps replays the fixture`() {
        for (case in fixture["sortAppsCases"]!!.jsonArray.map { it.jsonObject }) {
            val apps = case["apps"]!!.jsonArray.map { it.jsonObject }.map {
                Row(it["slug"]!!.jsonPrimitive.content, it["name"]!!.jsonPrimitive.content)
            }
            assertEquals(case["order"]!!.jsonArray.map { it.jsonPrimitive.content }, Apps.sortApps(apps).map { it.slug })
        }
    }

    @Test
    fun `the copy, preview and test ids match the fixture`() {
        val copy = fixture["copy"]!!.jsonObject
        fun c(key: String) = copy[key]!!.jsonPrimitive.content
        val menu = copy["menu"]!!.jsonObject
        val tabs = copy["tabs"]!!.jsonObject
        val expected = listOf(
            c("nav") to Apps.Copy.NAV, c("title") to Apps.Copy.TITLE, c("newApp") to Apps.Copy.NEW_APP,
            c("version") to Apps.Copy.VERSION, c("noVersion") to Apps.Copy.NO_VERSION,
            c("versions") to Apps.Copy.VERSIONS, c("head") to Apps.Copy.HEAD, c("noNote") to Apps.Copy.NO_NOTE,
            c("byAgent") to Apps.Copy.BY_AGENT, c("viewing") to Apps.Copy.VIEWING, c("restore") to Apps.Copy.RESTORE,
            c("restored") to Apps.Copy.RESTORED, c("restoredNote") to Apps.Copy.RESTORED_NOTE, c("empty") to Apps.Copy.EMPTY,
            c("chatEmpty") to Apps.Copy.CHAT_EMPTY, c("previewEmpty") to Apps.Copy.PREVIEW_EMPTY,
            c("composerPlaceholder") to Apps.Copy.COMPOSER_PLACEHOLDER, c("noDevice") to Apps.Copy.NO_DEVICE,
            c("namePlaceholder") to Apps.Copy.NAME_PLACEHOLDER, c("descriptionPlaceholder") to Apps.Copy.DESCRIPTION_PLACEHOLDER,
            c("create") to Apps.Copy.CREATE, c("rename") to Apps.Copy.RENAME, c("deleteTitle") to Apps.Copy.DELETE_TITLE,
            c("deleteBody") to Apps.Copy.DELETE_BODY, c("delete") to Apps.Copy.DELETE,
            menu["rename"]!!.jsonPrimitive.content to Apps.Copy.MENU_RENAME,
            menu["delete"]!!.jsonPrimitive.content to Apps.Copy.MENU_DELETE,
            tabs["chat"]!!.jsonPrimitive.content to Apps.Copy.TAB_CHAT,
            tabs["preview"]!!.jsonPrimitive.content to Apps.Copy.TAB_PREVIEW,
            tabs["versions"]!!.jsonPrimitive.content to Apps.Copy.TAB_VERSIONS,
            c("records") to Apps.Copy.RECORDS,
            c("recordsOne") to Apps.Copy.RECORDS_ONE, c("recordsOf") to Apps.Copy.RECORDS_OF,
            c("recordsNone") to Apps.Copy.RECORDS_NONE,
        )
        for ((want, got) in expected) assertEquals(want, got)

        // Exponential ships no built-in apps (2026-10-24).
        assertEquals(null, fixture["builtin"])
        assertEquals(null, copy["builtin"])

        val preview = fixture["preview"]!!.jsonObject
        assertEquals(preview["radius"]!!.jsonPrimitive.int, Apps.Preview.RADIUS)
        assertEquals(preview["padding"]!!.jsonPrimitive.int, Apps.Preview.PADDING)
        assertEquals(preview["surfaceId"]!!.jsonPrimitive.content, Apps.Preview.SURFACE_ID)

        val ids = fixture["testIds"]!!.jsonObject
        val expectedIds = mapOf(
            "list" to Apps.TestIds.LIST, "row" to Apps.TestIds.ROW, "newApp" to Apps.TestIds.NEW_APP,
            "editor" to Apps.TestIds.EDITOR, "chat" to Apps.TestIds.CHAT, "preview" to Apps.TestIds.PREVIEW,
            "versions" to Apps.TestIds.VERSIONS, "versionRow" to Apps.TestIds.VERSION_ROW, "tabs" to Apps.TestIds.TABS,
            "composer" to Apps.TestIds.COMPOSER, "viewing" to Apps.TestIds.VIEWING,
            "chatCard" to Apps.TestIds.CHAT_CARD, "dockToSide" to Apps.TestIds.DOCK_TO_SIDE, "float" to Apps.TestIds.FLOAT,
        )
        assertEquals(ids.keys, expectedIds.keys)
        for ((key, value) in expectedIds) assertEquals(ids[key]!!.jsonPrimitive.content, value)
    }

    @Test
    fun `the caps match the contract`() {
        val apps = contract["apps"]!!.jsonObject
        assertEquals(apps["slugMax"]!!.jsonPrimitive.int, Apps.SLUG_MAX)
        assertEquals(apps["nameMax"]!!.jsonPrimitive.int, Apps.NAME_MAX)
        assertEquals(apps["descriptionMax"]!!.jsonPrimitive.int, Apps.DESCRIPTION_MAX)
        assertEquals(apps["noteMax"]!!.jsonPrimitive.int, Apps.NOTE_MAX)
        assertEquals(apps["kindPattern"]!!.jsonPrimitive.content, Apps.KIND_PATTERN)
        assertEquals(apps["recordMaxBytes"]!!.jsonPrimitive.int, Apps.RECORD_MAX_BYTES)
        assertEquals(apps["recordKindsMax"]!!.jsonPrimitive.int, Apps.RECORD_KINDS_MAX)
    }

    @Test
    fun `the labels fill their slots`() {
        assertEquals("Viewing v3", Apps.viewing(3))
        assertEquals("Restored v2 as v5", Apps.restored(2, 5))
        assertEquals("Delete Team dashboard?", Apps.deleteTitle("Team dashboard"))
    }
}
