package com.exponential.app.ui.components

import com.exponential.app.data.api.SteerDevice
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1169: the join rule every invite-accept surface applies. */
class JoinDeviceStepRuleTest {

    @Test
    fun aCallerWithNoOwnDeviceGetsTheDevicesStep() {
        assertTrue(needsJoinDeviceStep(emptyList()))
    }

    @Test
    fun aCallerWhoOwnsADeviceJoinsAsBefore() {
        assertFalse(needsJoinDeviceStep(listOf(SteerDevice(deviceId = "mac"))))
    }

    @Test
    fun anUnresolvedListCountsAsOwningOneSoNobodyIsTrapped() {
        assertFalse(needsJoinDeviceStep(null))
    }
}
