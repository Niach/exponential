package com.exponential.app.domain

import com.exponential.app.data.db.IssueEventEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-900: the read-time activity fold, locked ×4 (web `fold.test.ts`, iOS
 * `ActivityFoldTests`, desktop `domain::activity_fold`) against the ONE
 * contract fixture — same cases, same names (each case's `name` is the
 * assertion message here, the way `IssueNestingTest` consumes its fixture).
 */
class ActivityFoldTest {

    /** The four fields a folded row is compared on — never the payload TEXT. */
    private data class Row(
        val id: String,
        val type: String,
        val payload: JsonElement?,
        val createdAt: String,
    )

    private data class FixtureCase(
        val name: String,
        val events: List<IssueEventEntity>,
        val barriers: List<ActivityFold.ActivityBarrier>,
        val expected: List<Row>,
    )

    private fun cases(): List<FixtureCase> =
        Json.parseToJsonElement(contractFixtureJson("activity-fold.json"))
            .jsonArray.map { element ->
            val case = element.jsonObject
            FixtureCase(
                name = case.getValue("name").jsonPrimitive.content,
                events = case.getValue("events").jsonArray.map { eventEntity(it.jsonObject) },
                barriers = case.getValue("barriers").jsonArray.map { barrier ->
                    val obj = barrier.jsonObject
                    ActivityFold.ActivityBarrier(
                        issueId = obj.getValue("issueId").jsonPrimitive.content,
                        actorUserId = obj.getValue("actorUserId").asNullableString(),
                        createdAt = obj.getValue("createdAt").jsonPrimitive.content,
                    )
                },
                expected = case.getValue("expected").jsonArray.map { row ->
                    val obj = row.jsonObject
                    Row(
                        id = obj.getValue("id").jsonPrimitive.content,
                        type = obj.getValue("type").jsonPrimitive.content,
                        payload = obj["payload"]?.takeIf { it !is JsonNull },
                        createdAt = obj.getValue("createdAt").jsonPrimitive.content,
                    )
                },
            )
        }

    @Test
    fun `every fixture case folds byte exact`() {
        val cases = cases()
        assertTrue(cases.size >= 20)
        for (case in cases) {
            assertEquals(
                case.name,
                case.expected,
                ActivityFold.foldActivity(case.events, case.barriers).map(::row),
            )
        }
    }

    @Test
    fun `the fixture covers every fold rule`() {
        val names = cases().map { it.name }
        fun covers(needle: String) = names.any { it.contains(needle) }
        assertTrue(covers("breaks the run"))
        assertTrue(covers("does not break the run"))
        assertTrue(covers("never fold"))
        assertTrue(covers("left alone"))
        assertTrue(covers("without an actor"))
        assertTrue(covers("empty list"))
    }

    @Test
    fun `a run exactly on the window boundary still folds`() {
        val start = "2026-09-19T10:00:00Z"
        fun at(offsetMs: Long): String =
            java.time.Instant.parse(start).plusMillis(offsetMs).toString()

        fun run(lastAt: String) = ActivityFold.foldActivity(
            listOf(
                statusEvent("e1", start, from = "a", to = "b"),
                statusEvent("e2", lastAt, from = "b", to = "c"),
            ),
        ).map { it.id }

        assertEquals(listOf("e2"), run(at(ActivityFold.FOLD_WINDOW_MS)))
        assertEquals(listOf("e1", "e2"), run(at(ActivityFold.FOLD_WINDOW_MS + 1)))
    }

    @Test
    fun `the input list and its rows are never mutated`() {
        val events = listOf(
            statusEvent("e1", "2026-09-19T10:00:00Z", from = "a", to = "b"),
            statusEvent("e2", "2026-09-19T10:01:00Z", from = "b", to = "c"),
        )
        val before = events.map { it.copy() }
        val folded = ActivityFold.foldActivity(events)
        assertEquals(before, events)
        assertEquals(1, folded.size)
        assertEquals("e2", folded.first().id)
        assertEquals(
            Json.parseToJsonElement("""{"fromStatusId":"a","toStatusId":"c"}"""),
            Json.parseToJsonElement(folded.first().payload!!),
        )
    }

    @Test
    fun `the postgres wire timestamp form folds like the iso form`() {
        val folded = ActivityFold.foldActivity(
            listOf(
                statusEvent("e1", "2026-09-19 10:00:00+00", from = "a", to = "b"),
                statusEvent("e2", "2026-09-19 10:01:00+00", from = "b", to = "a"),
            ),
        )
        assertEquals(emptyList<String>(), folded.map { it.id })
    }

    @Test
    fun `an unparseable timestamp never folds and stays in place`() {
        val folded = ActivityFold.foldActivity(
            listOf(
                statusEvent("e1", "2026-09-19T10:00:00Z", from = "a", to = "b"),
                statusEvent("e2", "not a timestamp", from = "b", to = "a"),
            ),
        )
        assertEquals(listOf("e1", "e2"), folded.map { it.id })
    }

    @Test
    fun `the field key names the one label or relation`() {
        assertEquals(
            "status",
            ActivityFold.foldFieldKey(statusEvent("e", "2026-09-19T10:00:00Z", "a", "b")),
        )
        assertEquals(
            "label:L",
            ActivityFold.foldFieldKey(
                event("e", "label_added", """{"labelId":"L"}""", "2026-09-19T10:00:00Z"),
            ),
        )
        assertEquals(
            "relation:blocks:i9",
            ActivityFold.foldFieldKey(
                event(
                    "e",
                    "relation_added",
                    """{"type":"blocks","relatedIssueId":"i9"}""",
                    "2026-09-19T10:00:00Z",
                ),
            ),
        )
        assertEquals(
            null,
            ActivityFold.foldFieldKey(event("e", "created", null, "2026-09-19T10:00:00Z")),
        )
    }

    private fun row(event: IssueEventEntity) = Row(
        id = event.id,
        type = event.type,
        payload = event.payload?.let { Json.parseToJsonElement(it) },
        createdAt = event.createdAt,
    )

    private fun statusEvent(id: String, createdAt: String, from: String, to: String) = event(
        id = id,
        type = "status_changed",
        payload = """{"fromStatusId":"$from","toStatusId":"$to"}""",
        createdAt = createdAt,
    )

    private fun event(
        id: String,
        type: String,
        payload: String?,
        createdAt: String,
        issueId: String = "i1",
        actorUserId: String? = "A",
    ) = IssueEventEntity(
        id = id,
        issueId = issueId,
        teamId = "t1",
        actorUserId = actorUserId,
        type = type,
        payload = payload,
        createdAt = createdAt,
        updatedAt = createdAt,
    )

    /** A fixture event row → the synced entity (payload rides as JSON text). */
    private fun eventEntity(obj: JsonObject) = event(
        id = obj.getValue("id").jsonPrimitive.content,
        type = obj.getValue("type").jsonPrimitive.content,
        payload = obj["payload"]?.takeIf { it !is JsonNull }?.toString(),
        createdAt = obj.getValue("createdAt").jsonPrimitive.content,
        issueId = obj.getValue("issueId").jsonPrimitive.content,
        actorUserId = obj.getValue("actorUserId").asNullableString(),
    )

    private fun JsonElement.asNullableString(): String? =
        if (this is JsonNull) null else jsonPrimitive.content
}
