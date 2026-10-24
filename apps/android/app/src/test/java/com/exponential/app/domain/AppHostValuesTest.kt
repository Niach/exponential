package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import com.exponential.app.data.db.DeviceEntity
import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Test

// VAPP-110: the app host's `exp:` values, derived like the web host's
// `devicesValue` (apps/web/src/lib/exponential-ui-host.tsx).
class AppHostValuesTest {
    private val now = Instant.parse("2026-10-22T12:00:00Z").toEpochMilli()

    private fun device(id: String, label: String, seen: String?, kind: String = "desktop") = DeviceEntity(
        id = id, userId = "u1", deviceId = "d-$id", label = label, kind = kind,
        platform = "macOS", version = "0.14.0", lastSeenAt = seen,
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
}
