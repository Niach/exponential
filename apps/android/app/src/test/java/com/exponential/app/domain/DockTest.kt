package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** N-12: [Dock] replays `fixtures/dock.json` (rowCases, cases, autoOpenCases), comparing WHOLE objects. */
class DockTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("dock.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun array(key: String): List<JsonObject> = fixture.getValue(key).jsonArray.map { it.jsonObject }

    private fun nullable(value: String?): JsonElement = value?.let { JsonPrimitive(it) } ?: JsonNull

    private fun kind(wire: String) = Dock.Kind.entries.first { it.wire == wire }

    private fun tint(wire: String) = Dock.Tint.entries.first { it.wire == wire }

    private fun section(json: JsonObject?): TaskQueueSection? = json?.let { s ->
        TaskQueueSection(
            title = s.str("title")!!,
            icon = "",
            items = (s["items"] as? JsonArray).orEmpty().mapIndexed { i, item ->
                val o = item.jsonObject
                TaskQueueItem(
                    key = "task:$i",
                    kind = TaskQueueItemKind.Task,
                    state = TaskQueueItemState.entries.first { it.wire == o.str("state") },
                    icon = "",
                    label = o.str("label")!!,
                    description = o.str("description"),
                )
            },
        )
    }

    private fun source(json: JsonObject): Dock.Source = when (json.str("kind")) {
        "plan", "question" -> Dock.Source.Ask(
            id = json.str("id")!!,
            kind = AskDock.Kind.entries.first { it.wire == json.str("kind") },
            text = json.str("text")!!,
            sessionName = json.str("sessionName"),
            questions = json["questions"]?.jsonPrimitive?.int ?: 0,
            createdAt = json.getValue("createdAt").jsonPrimitive.long,
        )
        "tasks" -> Dock.Source.Tasks(
            (json["queue"] as? JsonObject)?.let { q ->
                TaskQueue(section(q.getValue("todo").jsonObject)!!, section(q["completed"] as? JsonObject))
            },
        )
        "queued" -> Dock.Source.Queued(
            json.getValue("messages").jsonArray.map { Dock.QueuedMessage(it.jsonObject.str("id")!!, it.jsonObject.str("text")!!) },
        )
        else -> Dock.Source.Banner(
            id = json.str("id")!!,
            icon = json.str("icon")!!,
            label = json.str("label")!!,
            text = json.str("text")!!,
            tone = tint(json.str("tone")!!),
            action = json.str("action"),
        )
    }

    private fun row(json: JsonObject) = Dock.Row(
        id = json.str("id")!!,
        kind = kind(json.str("kind")!!),
        tint = tint(json.str("tint")!!),
        icon = json.str("icon")!!,
        label = json.str("label")!!,
        caption = json.str("caption"),
        meta = json.str("meta"),
        title = json.str("title")!!,
        expandable = json.getValue("expandable").jsonPrimitive.boolean,
        action = json.str("action"),
    )

    private fun rowJson(row: Dock.Row) = buildJsonObject {
        put("id", row.id)
        put("kind", row.kind.wire)
        put("tint", row.tint.wire)
        put("icon", row.icon)
        put("label", row.label)
        put("caption", nullable(row.caption))
        put("meta", nullable(row.meta))
        put("title", row.title)
        put("expandable", row.expandable)
        put("action", nullable(row.action))
    }

    @Test
    fun `constants match the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        val queued = copy.getValue("queued").jsonObject
        assertEquals(queued.str("one"), Dock.Copy.QUEUED_ONE)
        assertEquals(queued.str("many"), Dock.Copy.QUEUED_MANY)
        val questions = copy.getValue("questions").jsonObject
        assertEquals(questions.str("one"), Dock.Copy.QUESTIONS_ONE)
        assertEquals(questions.str("many"), Dock.Copy.QUESTIONS_MANY)
        assertEquals(copy.str("close"), Dock.Copy.CLOSE)
        assertEquals(copy.str("showAll"), Dock.Copy.SHOW_ALL)
        val icons = fixture.getValue("icons").jsonObject.mapValues { it.value.jsonPrimitive.content }
        assertEquals(
            mapOf(
                "plan" to Dock.Icons.PLAN,
                "question" to Dock.Icons.QUESTION,
                "tasks" to Dock.Icons.TASKS,
                "queued" to Dock.Icons.QUEUED,
                "open" to Dock.Icons.OPEN,
                "close" to Dock.Icons.CLOSE,
            ),
            icons,
        )
        val ids = fixture.getValue("testIds").jsonObject.mapValues { it.value.jsonPrimitive.content }
        assertEquals(
            mapOf(
                "dock" to Dock.TestIds.DOCK,
                "pile" to Dock.TestIds.PILE,
                "card" to Dock.TestIds.CARD,
                "sheet" to Dock.TestIds.SHEET,
                "sheetBody" to Dock.TestIds.SHEET_BODY,
                "sheetFooter" to Dock.TestIds.SHEET_FOOTER,
                "close" to Dock.TestIds.CLOSE,
            ),
            ids,
        )
        val stack = fixture.getValue("stack").jsonObject
        assertEquals(stack.getValue("peek").jsonPrimitive.double, Dock.STACK.peek, 0.0)
        assertEquals(stack.getValue("scaleStep").jsonPrimitive.double, Dock.STACK.scaleStep, 0.0)
        assertEquals(stack.getValue("visible").jsonPrimitive.int, Dock.STACK.visible)
        assertEquals(stack.getValue("gap").jsonPrimitive.double, Dock.STACK.gap, 0.0)
        val sheet = fixture.getValue("sheet").jsonObject
        assertEquals(sheet.getValue("maxHeightFraction").jsonPrimitive.double, Dock.SHEET_MAX_HEIGHT_FRACTION.toDouble(), 1e-6)
        assertEquals(sheet.getValue("radius").jsonPrimitive.int, Dock.SHEET_RADIUS)
        assertEquals(fixture.getValue("order").jsonArray.map { it.jsonPrimitive.content }, Dock.ORDER)
    }

    @Test
    fun `row cases`() {
        val cases = array("rowCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val rows = Dock.dockRows(case.getValue("sources").jsonArray.map { source(it.jsonObject) })
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(rows.map(::rowJson)))
        }
    }

    @Test
    fun `dock cases`() {
        val cases = array("cases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val view = Dock.dock(
                rows = input.getValue("rows").jsonArray.map { row(it.jsonObject) },
                expanded = input.getValue("expanded").jsonPrimitive.boolean,
                openId = input.str("openId"),
            )
            val actual = buildJsonObject {
                put("mode", view.mode.wire)
                put("frontId", nullable(view.frontId))
                put("openId", nullable(view.openId))
                put("hidden", view.hidden)
                put("rows", JsonArray(view.rows.map(::rowJson)))
            }
            assertEquals(case.str("name"), case.getValue("expected"), actual)
        }
    }

    @Test
    fun `auto-open cases`() {
        val cases = array("autoOpenCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val actual = Dock.dockAutoOpen(
                previous = case.getValue("previous").jsonArray.map { it.jsonPrimitive.content },
                rows = case.getValue("rowIds").jsonArray.map {
                    val pair = it.jsonArray
                    pair[0].jsonPrimitive.content to kind(pair[1].jsonPrimitive.content)
                },
            )
            assertEquals(case.str("name"), case.str("expected"), actual)
        }
    }

    @Test
    fun `the pile shares the toast geometry`() {
        val layout = ToastStack.stackLayout(listOf(60.0, 60.0, 60.0, 60.0), expanded = false, anchoredBottom = true, Dock.STACK)
        assertEquals(102.0, layout.height, 1e-9)
        assertEquals(listOf(0.9, 0.9, 0.95, 1.0), layout.items.map { it.scale })
        assertEquals(listOf(false, true, true, true), layout.items.map { it.visible })
        val fanned = ToastStack.stackLayout(listOf(60.0, 60.0), expanded = true, anchoredBottom = true, Dock.STACK)
        assertEquals(126.0, fanned.height, 1e-9)
        assertEquals(listOf(0.0, 66.0), fanned.items.map { it.offset })
    }
}
