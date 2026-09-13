package com.exponential.app.ui.session

import com.exponential.app.domain.AgentHealth
import com.exponential.app.domain.DeviceAccountChip
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

/**
 * The Devices page's per-machine command slots. EXP-862 retired the Accounts
 * scroll index this file was named for: the device-settings sheet carries no
 * accounts (and so no Usage button) any more, so nothing counts list items.
 */
class AccountsHeaderIndexTest {

    @Test
    fun `a machine chip's command slot is scoped to its machine`() {
        // EXP-849: the chip key is only `<agent>:<profileId>`, so two machines
        // holding the SAME login would share one spinner and one error caption
        // without the device scope.
        val chip = DeviceAccountChip(
            key = "claude:system",
            agent = "claude",
            profileId = "system",
            profileLabel = "Default",
            signedIn = true,
            active = true,
            email = "a@acme.test",
            plan = null,
            health = AgentHealth.Ok,
        )
        assertEquals("studio:claude:system", deviceAccountCommandKey("studio", chip))
        assertNotEquals(
            deviceAccountCommandKey("studio", chip),
            deviceAccountCommandKey("buildbox", chip),
        )
    }

    @Test
    fun `an account row's chip lands in the same slot as the machine row's`() {
        // EXP-862: both surfaces carry the same chip menu, so a command fired
        // from either must caption in ONE place.
        assertEquals(
            deviceAccountCommandKey(
                "studio",
                DeviceAccountChip(
                    key = "claude:work",
                    agent = "claude",
                    profileId = "work",
                    profileLabel = "Work",
                    signedIn = true,
                    active = false,
                    email = null,
                    plan = null,
                    health = AgentHealth.Ok,
                ),
            ),
            accountCommandKey("studio", "claude", "work"),
        )
    }
}
