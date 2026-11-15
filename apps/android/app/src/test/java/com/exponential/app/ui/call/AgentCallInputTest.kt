package com.exponential.app.ui.call

import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.domain.Voice
import com.exponential.app.domain.VoiceCall
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1857: the phone's `callTarget` input (web `webCallInput`). */
class AgentCallInputTest {

    private val now = 1_800_000_000_000L
    private fun iso(ms: Long) = java.time.Instant.ofEpochMilli(ms).toString()

    private fun row(id: String, user: String, label: String, seenAgoMs: Long, kind: String = "desktop") =
        DeviceEntity(id = "row-$id", userId = user, deviceId = id, label = label, kind = kind, lastSeenAt = iso(now - seenAgoMs))

    @Test
    fun `own desktops only, newest first, the meeting on the voice device`() {
        val rows = listOf(
            row("d-mint", "me", "mint", 600_000),
            row("d-mac", "me", "macbook", 5_000),
            row("d-theirs", "them", "theirs", 1_000),
            row("d-server", "me", "server", 1_000, kind = "server"),
        )
        val settings = Voice.Settings(voiceDeviceId = "d-mint", meetingStartedAt = iso(now))
        val input = AgentCallViewModel.phoneCallInput(settings, "d-mint", rows, "me", now)
        assertEquals(VoiceCall.Platform.Phone, input.platform)
        assertEquals(null, input.here)
        assertEquals("d-mint", input.agentDevice)
        assertEquals(
            listOf(
                VoiceCall.TargetDevice("d-mac", "macbook", online = true, speech = null, meeting = false),
                VoiceCall.TargetDevice("d-mint", "mint", online = false, speech = null, meeting = true),
            ),
            input.desktops,
        )
    }

    @Test
    fun `settings not loaded yet read as no desktop`() {
        val input = AgentCallViewModel.phoneCallInput(null, "d-mint", listOf(row("d-mint", "me", "mint", 0)), "me", now)
        assertTrue(input.desktops.isEmpty())
        assertEquals(VoiceCall.Copy.Reasons.NO_DESKTOP, VoiceCall.callTarget(input).reason)
    }
}
