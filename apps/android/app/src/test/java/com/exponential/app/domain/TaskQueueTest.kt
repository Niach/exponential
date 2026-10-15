package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1321/1324: [TaskQueues.of] replays `fixtures/task-queue.json` and
 * compares the WHOLE expected queue (web, desktop `steer::task_queue` and iOS
 * `TaskQueueTests` replay the same cases).
 */
class TaskQueueTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("task-queue.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.lng(key: String): Long? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.long

    private fun JsonObject.list(key: String): List<JsonObject> =
        this[key]?.takeUnless { it is JsonNull }?.jsonArray?.map { it.jsonObject }.orEmpty()

    private fun JsonObject.obj(key: String): JsonObject? =
        this[key]?.takeUnless { it is JsonNull }?.jsonObject

    private fun section(json: JsonObject?): TaskQueueSection? = json?.let { s ->
        TaskQueueSection(
            title = s.str("title")!!,
            icon = s.str("icon")!!,
            items = s.list("items").map {
                TaskQueueItem(
                    key = it.str("key")!!,
                    kind = TaskQueueItemKind.entries.first { k -> k.wire == it.str("kind") },
                    state = TaskQueueItemState.entries.first { k -> k.wire == it.str("state") },
                    icon = it.str("icon")!!,
                    label = it.str("label")!!,
                    description = it.str("description"),
                )
            },
        )
    }

    @Test
    fun `every fixture case builds the whole expected queue`() {
        val cases = fixture.list("cases")
        assertTrue(cases.size >= 6)
        for (case in cases) {
            val input = case.obj("input")!!
            val actual = TaskQueues.of(
                TaskQueueInput(
                    tasks = input.list("tasks").map { TaskQueueTaskInput(it.str("content")!!, it.str("status")!!) },
                    backgroundTasks = input.list("backgroundTasks").map {
                        TaskQueueBackgroundInput(it.str("id")!!, it.str("kind")!!, it.str("description")!!, it.lng("startedAt"))
                    },
                    waits = input.list("waits").map { TaskQueueWaitInput(it.str("id")!!, it.str("subject")!!) },
                    now = input.lng("now")!!,
                ),
            )
            val expected = case.obj("expected")?.let {
                TaskQueue(todo = section(it.obj("todo"))!!, completed = section(it.obj("completed")))
            }
            assertEquals(case.str("name"), expected, actual)
        }
    }

    @Test
    fun `the spec constants match the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("todo"), TaskQueueSpec.TODO)
        assertEquals(copy.str("completed"), TaskQueueSpec.COMPLETED)
        assertEquals(copy.str("waiting"), TaskQueueSpec.WAITING)
        val icons = fixture.getValue("icons").jsonObject
        assertEquals(icons.str("todo"), TaskQueueSpec.ICON_TODO)
        assertEquals(icons.str("completed"), TaskQueueSpec.ICON_COMPLETED)
        assertEquals(icons.str("running"), TaskQueueSpec.ICON_RUNNING)
        assertEquals(icons.str("pending"), TaskQueueSpec.ICON_PENDING)
        assertEquals(icons.str("done"), TaskQueueSpec.ICON_DONE)
        assertEquals(icons.str("background"), TaskQueueSpec.ICON_BACKGROUND)
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.str("queue"), TaskQueueSpec.TEST_ID_QUEUE)
        assertEquals(ids.str("todo"), TaskQueueSpec.TEST_ID_TODO)
        assertEquals(ids.str("completed"), TaskQueueSpec.TEST_ID_COMPLETED)
    }
}
