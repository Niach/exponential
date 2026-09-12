package com.exponential.app.ui.session

import com.exponential.app.domain.AgentHealth
import com.exponential.app.domain.DeviceAccountChip
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

/**
 * EXP-827: the Devices list index the device sheet's round Usage button scrolls
 * to. It is counted off the list above it, so it is the one thing that silently
 * drifts when an item moves — these cases pin the four shapes the list has.
 */
class AccountsHeaderIndexTest {

    @Test
    fun `the accounts header sits under the machine groups`() {
        // Still loading: "My machines" + the spacer.
        assertEquals(2, accountsHeaderIndex(ownCount = null, teamCount = 0))
        // No machines: the header, the hint row, the spacer.
        assertEquals(3, accountsHeaderIndex(ownCount = 0, teamCount = 0))
        assertEquals(4, accountsHeaderIndex(ownCount = 2, teamCount = 0))
        // A shared server adds its own header plus its rows.
        assertEquals(7, accountsHeaderIndex(ownCount = 2, teamCount = 2))
        assertEquals(5, accountsHeaderIndex(ownCount = 0, teamCount = 1))
    }

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
}
