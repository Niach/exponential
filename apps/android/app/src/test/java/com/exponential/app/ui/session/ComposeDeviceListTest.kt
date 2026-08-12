package com.exponential.app.ui.session

import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.UserEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-481: the synced devices rows → the Agents tab's list. Own machines
 * first (last-seen desc), then ONLY the selected team's shared servers with
 * their owner resolved — a machine shared with a DIFFERENT common team must
 * not appear under this team.
 */
class ComposeDeviceListTest {

    private val nowMs = 1_754_900_000_000L

    private fun device(
        id: String,
        userId: String,
        lastSeen: String,
        kind: String = "server",
        sharedTeamId: String? = null,
    ) = DeviceEntity(
        id = id,
        userId = userId,
        deviceId = "steer-$id",
        label = id,
        kind = kind,
        lastSeenAt = lastSeen,
        sharedTeamId = sharedTeamId,
    )

    private val users = listOf(
        UserEntity(
            id = "them",
            name = "Danny",
            email = "d@example.com",
            createdAt = "2026-01-01T00:00:00Z",
            updatedAt = "2026-01-01T00:00:00Z",
        ),
    )

    @Test
    fun `own rows first (last-seen desc), then the selected team's shared servers`() {
        val rows = listOf(
            device("old-mine", "me", "2026-08-01T10:00:00Z"),
            device("new-mine", "me", "2026-08-11T10:00:00Z"),
            device("shared", "them", "2026-08-11T09:00:00Z", sharedTeamId = "team-1"),
            device("other-team", "them", "2026-08-11T09:00:00Z", sharedTeamId = "team-2"),
            // A desktop share must never scope in (server-kind only).
            device("shared-desktop", "them", "2026-08-11T09:00:00Z", kind = "desktop", sharedTeamId = "team-1"),
        )
        val list = composeDeviceList(rows, users, "team-1", "me", nowMs)
        assertEquals(
            listOf("steer-new-mine", "steer-old-mine", "steer-shared"),
            list.map { it.deviceId },
        )
        assertEquals("Danny", list.last().owner?.name)
        assertTrue(list.first().isMine)
    }

    @Test
    fun `no selected team lists only own rows and signed out lists nothing`() {
        val rows = listOf(
            device("mine", "me", "2026-08-11T10:00:00Z"),
            device("shared", "them", "2026-08-11T09:00:00Z", sharedTeamId = "team-1"),
        )
        assertEquals(
            listOf("steer-mine"),
            composeDeviceList(rows, users, null, "me", nowMs).map { it.deviceId },
        )
        assertTrue(composeDeviceList(rows, users, "team-1", null, nowMs).isEmpty())
    }
}
