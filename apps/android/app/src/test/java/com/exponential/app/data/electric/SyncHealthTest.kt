package com.exponential.app.data.electric

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The offline banner's decision table (EXP-533), locked byte-for-byte against
 * the desktop `crates/sync/src/health.rs` suite it was ported from — the two
 * models must agree, or the same outage reads differently on two clients.
 *
 * The rule: alarm only for a failure streak that PERSISTED, never for the
 * simultaneous burst of failures every wake produces, and never for an error
 * that stopped repeating long ago (the phone slept through the outage).
 */
class SyncHealthTest {

    private fun at(seconds: Long) = seconds * 1_000L

    private fun AccountHealth.err(seconds: Long) = recordFailure(at(seconds), "http 500")

    @Test
    fun noErrorIsOk() {
        assertEquals(SyncHealth.Ok, AccountHealth().health(at(1_000)))
        assertEquals(SyncHealth.Ok, AccountHealth().recordSuccess(at(500)).health(at(1_000)))
    }

    @Test
    fun failureWithinGraceIsOk() {
        val h = AccountHealth().err(1_000)
        assertEquals(SyncHealth.Ok, h.health(at(1_005)))
        assertEquals(SyncHealth.Ok, h.health(at(1_011)))
    }

    @Test
    fun streakPastGraceIsOffline() {
        val h = AccountHealth().err(1_000).err(1_010)
        assertEquals(SyncHealth.Offline, h.health(at(1_012)))
    }

    @Test
    fun successAfterErrorClearsInstantly() {
        val h = AccountHealth().err(1_000).err(1_020)
        assertEquals(SyncHealth.Offline, h.health(at(1_020)))
        val recovered = h.recordSuccess(at(1_021))
        assertEquals(SyncHealth.Ok, recovered.health(at(1_021)))
        assertFalse(recovered.streakOpen)
    }

    @Test
    fun errorPastStalenessWindowIsOk() {
        // The streak persisted past the grace, but the phone then slept: on
        // wake the hours-old error must not alarm.
        val h = AccountHealth().err(1_000).err(1_020)
        assertEquals(SyncHealth.Offline, h.health(at(1_030)))
        assertEquals(SyncHealth.Ok, h.health(at(1_020 + 300)))
    }

    @Test
    fun gapPastStalenessRestartsStreak() {
        // First fresh failure after a sleep-sized gap: a new streak with a
        // fresh grace window, not an instant alarm off the old streak start.
        val h = AccountHealth().err(1_000).err(1_020).err(1_020 + 400)
        assertEquals(at(1_420), h.failureStreakStartedAtMs)
        assertEquals(SyncHealth.Ok, h.health(at(1_421)))
        // ...and if the failures keep coming, the new streak alarms normally.
        assertEquals(SyncHealth.Offline, h.err(1_430).health(at(1_433)))
    }

    @Test
    fun successResetsGraceForNextFailure() {
        val h = AccountHealth().err(1_000).err(1_015).recordSuccess(at(1_016))
        // A lone new failure starts a fresh streak — grace applies again.
        val next = h.err(1_017)
        assertEquals(at(1_017), next.failureStreakStartedAtMs)
        assertEquals(SyncHealth.Ok, next.health(at(1_020)))
        assertEquals(SyncHealth.Offline, next.health(at(1_030)))
    }

    // Wall clock, so NTP (or the user) can move it backwards under a live
    // streak. A negative elapsed must read as "no time has passed" — never
    // wrap into an instant alarm or a false all-clear.
    @Test
    fun clockSkewSaturatesToZero() {
        val h = AccountHealth().err(1_000).err(1_020)
        assertEquals(SyncHealth.Ok, h.health(at(900)))
        assertEquals(SyncHealth.Ok, AccountHealth().err(1_000).health(at(1)))
    }

    @Test
    fun streakOpenOnlyWhileFailing() {
        assertFalse(AccountHealth().streakOpen)
        assertTrue(AccountHealth().err(1_000).streakOpen)
        assertFalse(AccountHealth().err(1_000).recordSuccess(at(1_001)).streakOpen)
    }

    // Locked because the banner's whole point is being slower than one failed
    // poll: every shape long-poll fails at once on wake, so a count-based rule
    // would flash the banner on a healthy server.
    @Test
    fun windowsMatchTheDesktopModel() {
        assertEquals(12_000L, FAILURE_STREAK_GRACE_MS)
        assertEquals(300_000L, ERROR_STALENESS_WINDOW_MS)
    }
}
