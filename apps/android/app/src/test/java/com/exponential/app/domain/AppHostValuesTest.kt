package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.IssueEntity
import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Test

// VAPP-110: the app host's `exp:` values, derived like the web host's
// `devicesValue` / `issuesSource` (apps/web/src/lib/exponential-ui-host.tsx).
class AppHostValuesTest {
    private val now = Instant.parse("2026-10-22T12:00:00Z").toEpochMilli()

    private fun device(id: String, label: String, seen: String?, kind: String = "desktop") = DeviceEntity(
        id = id, userId = "u1", deviceId = "d-$id", label = label, kind = kind,
        platform = "macOS", version = "0.14.0", lastSeenAt = seen,
    )

    private fun issue(id: String, board: String, order: Double) = IssueEntity(
        id = id, boardId = board, number = 1, identifier = "APP-$id", title = "Issue $id",
        status = "backlog", priority = "none", creatorId = "u1", sortOrder = order,
        createdAt = "", updatedAt = "",
    )

    @Test
    fun `devices sort by label, online within the window, else last seen`() {
        val value = AppHostValues.devices(
            listOf(
                device("b", "Zed box", "2026-10-22T11:00:00Z", kind = "server"),
                device("a", "Alex's MacBook Pro", "2026-10-22T11:59:40Z"),
                device("c", "Never", null),
            ),
            now,
        )
        val rows = value["rows"]!!.array!!
        assertEquals(listOf("Alex's MacBook Pro", "Never", "Zed box"), rows.map { it["label"]!!.string })
        assertEquals("Online", rows[0]["status"]!!.string)
        assertEquals("live", rows[0]["tone"]!!.string)
        assertEquals("success", rows[0]["discTone"]!!.string)
        assertEquals("Desktop · macOS · v0.14.0", rows[0]["detail"]!!.string)
        assertEquals("Offline", rows[1]["status"]!!.string)
        assertEquals("Last seen 1 h ago", rows[2]["status"]!!.string)
        assertEquals("Server · macOS · v0.14.0", rows[2]["detail"]!!.string)
        assertEquals("server", rows[2]["icon"]!!.string)
        assertEquals(JsonValue.Num(3.0), value["count"])
        assertEquals(JsonValue.Num(1.0), value["online"])
    }

    @Test
    fun `issues filter by board, keep board order and cap at the limit`() {
        val all = listOf(issue("3", "b1", 3.0), issue("1", "b1", 1.0), issue("2", "b2", 2.0), issue("4", "b1", 4.0))
        val value = AppHostValues.issues(all, mapOf("board" to "b1", "limit" to "2"))
        assertEquals(listOf("APP-1", "APP-3"), value["rows"]!!.array!!.map { it["identifier"]!!.string })
        assertEquals(JsonValue.Num(2.0), value["count"])
        assertEquals(4, AppHostValues.issues(all, emptyMap())["rows"]!!.array!!.size)
    }
}
