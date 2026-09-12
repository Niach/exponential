package com.exponential.app.ui.session

import org.junit.Assert.assertEquals
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
}
