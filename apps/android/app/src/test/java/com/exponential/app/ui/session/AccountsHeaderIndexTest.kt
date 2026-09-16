package com.exponential.app.ui.session

import com.exponential.app.domain.AgentHealth
import com.exponential.app.domain.AgentProfileUsageRow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

/**
 * The Devices page's per-machine command slots. EXP-862 retired the Accounts
 * scroll index this file was named for (the device-settings sheet carries no
 * accounts, and so no Usage button, any more), and EXP-909 retired the
 * cross-device Accounts section itself — a command is fired from ONE login row
 * under ONE machine now, so the slot is keyed off that row.
 */
class AccountsHeaderIndexTest {

    private fun login(deviceId: String, agent: String, profileId: String) = AgentProfileUsageRow(
        key = "$deviceId:$agent:$profileId",
        deviceId = deviceId,
        deviceLabel = deviceId,
        mine = true,
        online = true,
        agent = agent,
        profileId = profileId,
        profileLabel = "Default",
        active = true,
        signedIn = true,
        health = AgentHealth.Ok,
        email = "a@acme.test",
        plan = null,
        usage = null,
        checkedAt = null,
    )

    @Test
    fun `a login row's command slot is scoped to its machine`() {
        // EXP-849: two machines holding the SAME login would share one spinner
        // and one error caption without the device scope.
        val studio = login("studio", "claude", "system")
        assertEquals("studio:claude:system", deviceLoginCommandKey(studio))
        assertNotEquals(
            deviceLoginCommandKey(studio),
            deviceLoginCommandKey(login("buildbox", "claude", "system")),
        )
        // …and two logins on ONE machine never share one either.
        assertNotEquals(
            deviceLoginCommandKey(studio),
            deviceLoginCommandKey(login("studio", "claude", "work")),
        )
    }

    @Test
    fun `a login row lands in the same slot as its parts`() {
        // Both spellings address ONE slot, so a command fired from either
        // captions in one place.
        assertEquals(
            deviceLoginCommandKey(login("studio", "claude", "work")),
            accountCommandKey("studio", "claude", "work"),
        )
    }

    @Test
    fun `two machines' sign-ins for the same agent never share a slot`() {
        // A login's poll outlives its sheet: keyed per agent alone, machine
        // A's late link landed in machine B's sheet.
        assertNotEquals(
            agentLoginCommandKey("a", "claude", null),
            agentLoginCommandKey("b", "claude", null),
        )
        // …nor two logins on one machine, nor a new profile and the ambient one.
        assertNotEquals(
            agentLoginCommandKey("a", "claude", "work"),
            agentLoginCommandKey("a", "claude", "system"),
        )
        assertNotEquals(
            agentLoginCommandKey("a", "claude", null, newProfileLabel = "dev@acme.test"),
            agentLoginCommandKey("a", "claude", null),
        )
        // Null profile = the ambient login, the same slot as naming it.
        assertEquals(
            agentLoginCommandKey("a", "claude", "system"),
            agentLoginCommandKey("a", "claude", null),
        )
        // The code slot is the same sign-in under its own prefix.
        assertEquals("login:a:claude:work", agentLoginCommandKey("a", "claude", "work"))
        assertEquals("login-code:a:claude:work", agentLoginCodeCommandKey("a", "claude", "work"))
    }
}
