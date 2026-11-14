package com.exponential.app.domain

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// #EXP-1832: replays EVERY list of `packages/domain-contract/fixtures/
// sidebar-folders.json` (TS `sidebar-folders.test.ts`, desktop
// `domain::sidebar_folders`, iOS `SidebarFoldersTests`), comparing whole values.
class SidebarFoldersTest {
    private val fixture: JsonObject = run {
        val relative = "packages/domain-contract/fixtures/sidebar-folders.json"
        val candidates = listOf("../../../$relative", "../../$relative", relative)
        val file = candidates.map(::File).firstOrNull { it.isFile } ?: error("$relative not found from ${File(".").absolutePath}")
        Json.parseToJsonElement(file.readText()).jsonObject
    }

    private fun JsonElement.str(): String? = (this as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content

    private fun cases(key: String): List<JsonObject> = fixture.getValue(key).jsonArray.map { it.jsonObject }

    private fun JsonObject.items(): List<String> = getValue("items").jsonArray.map { it.jsonPrimitive.content }

    private fun JsonObject.entries(): List<SidebarFolders.Entry> = getValue("entries").jsonArray.map {
        val o = it.jsonObject
        SidebarFolders.Entry(
            id = o.getValue("id").jsonPrimitive.content,
            kind = o.getValue("kind").jsonPrimitive.content,
            targetId = o["targetId"]?.str(),
            parentId = o["parentId"]?.str(),
            name = o["name"]?.str(),
            sortOrder = o.getValue("sortOrder").jsonPrimitive.doubleOrNull!!,
        )
    }

    /** JSON → plain values, numbers as doubles, so `1` and `1.0` agree. */
    private fun plain(element: JsonElement): Any? = when (element) {
        is JsonNull -> null
        is JsonObject -> element.mapValues { plain(it.value) }
        is JsonArray -> element.map(::plain)
        is JsonPrimitive -> when {
            element.isString -> element.content
            element.booleanOrNull != null -> element.booleanOrNull
            else -> element.doubleOrNull
        }
    }

    private fun node(node: SidebarFolders.Node): Map<String, Any?> = when (node) {
        is SidebarFolders.Node.Item -> mapOf("kind" to "item", "id" to node.id)
        is SidebarFolders.Node.Folder -> mapOf("kind" to "folder", "id" to node.id, "name" to node.name, "children" to node.children)
    }

    private fun writes(writes: SidebarFolders.Writes?): Map<String, Any?>? = writes?.let { w ->
        mapOf(
            "upserts" to w.upserts.map {
                mapOf("id" to it.id, "kind" to it.kind, "targetId" to it.targetId, "parentId" to it.parentId, "name" to it.name, "sortOrder" to it.sortOrder)
            },
            "deletes" to w.deletes,
            "createdFolderId" to w.createdFolderId,
        )
    }

    @Test
    fun `the constants match the fixture`() {
        assertEquals(fixture.getValue("maxDepth").jsonPrimitive.int, SidebarFolders.MAX_DEPTH)
        assertEquals(fixture.getValue("defaultFolderName").jsonPrimitive.content, SidebarFolders.DEFAULT_FOLDER_NAME)
    }

    @Test
    fun `tree replays every case`() {
        val cases = cases("tree")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val actual = SidebarFolders.tree(case.items(), case.entries()).map(::node)
            assertEquals(case.getValue("name").jsonPrimitive.content, plain(case.getValue("expected")), actual)
        }
    }

    @Test
    fun `move replays every case`() {
        val cases = cases("move")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val m = case.getValue("move").jsonObject
            val move = SidebarFolders.Move(
                id = m.getValue("id").jsonPrimitive.content,
                before = m["before"]?.str(),
                after = m["after"]?.str(),
                into = m["into"]?.str(),
            )
            val actual = writes(SidebarFolders.move(case.items(), case.entries(), move, case.getValue("newFolderId").jsonPrimitive.content))
            assertEquals(case.getValue("name").jsonPrimitive.content, plain(case.getValue("expected")), actual)
        }
    }

    @Test
    fun `ungroup replays every case`() {
        val cases = cases("ungroup")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val actual = writes(SidebarFolders.ungroup(case.items(), case.entries(), case.getValue("folderId").jsonPrimitive.content))
            assertEquals(case.getValue("name").jsonPrimitive.content, plain(case.getValue("expected")), actual)
        }
    }

    @Test
    fun `rows replay every case`() {
        val cases = cases("rows")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val closed = case.getValue("closed").jsonArray.map { it.jsonPrimitive.content }.toSet()
            val tree = SidebarFolders.tree(case.items(), case.entries())
            val actual = SidebarFolders.rows(tree) { it !in closed }.map {
                mapOf("kind" to it.kind, "id" to it.id, "depth" to it.depth.toDouble(), "group" to it.group, "name" to it.name, "open" to it.open, "count" to it.count.toDouble())
            }
            assertEquals(case.getValue("name").jsonPrimitive.content, plain(case.getValue("expected")), actual)
        }
    }

    @Test
    fun `folder names replay every case`() {
        for (case in cases("folderNames")) {
            assertEquals(case.getValue("expected").jsonPrimitive.content, SidebarFolders.folderName(case.getValue("input").jsonPrimitive.content))
        }
    }

    @Test
    fun `open keys replay every case`() {
        val cases = cases("openKeys")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(
                case.getValue("expected").jsonPrimitive.content,
                SidebarFolders.openKey(case.getValue("teamId").jsonPrimitive.content, case.getValue("folderId").jsonPrimitive.content),
            )
        }
    }
}
