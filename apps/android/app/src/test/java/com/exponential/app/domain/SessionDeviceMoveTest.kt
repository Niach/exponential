package com.exponential.app.domain

import com.exponential.app.data.api.DeviceOwner
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.SteerDevice
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1252: the session composer's Device pin — rows, refusals, picks, copy. */
class SessionDeviceMoveTest {

    private val team = "team-1"

    private fun device(
        id: String,
        online: Boolean = true,
        shared: Boolean = false,
        agents: List<String>? = listOf("claude"),
        owner: DeviceOwner? = null,
        version: String? = "0.14.73",
    ) = SteerDevice(
        deviceId = id,
        deviceLabel = id.uppercase(),
        agents = agents,
        online = online,
        version = version,
        sharedTeamIds = if (shared) listOf(team) else emptyList(),
        owner = owner,
    )

    @Test
    fun copyIsByteLocked() {
        assertEquals("Move to device", SessionDeviceMove.PICKER_TITLE)
        assertEquals("Moving to {device}…", SessionDeviceMove.MOVING_LABEL)
        assertEquals("Moving to Studio…", SessionDeviceMove.movingLabel("Studio"))
        assertEquals("Couldn't move the session", SessionDeviceMove.MOVE_FAILED_TOAST)
        assertEquals("This device is offline.", SessionDeviceMove.REASON_OFFLINE)
        assertEquals("Agents move from Settings → Agents.", SessionDeviceMove.REASON_AGENT)
        assertEquals("This device needs an update to receive sessions.", SessionDeviceMove.REASON_UPDATE)
    }

    @Test
    fun pinHiddenForAgentsAndAsides() {
        assertEquals(SessionDeviceMove.REASON_AGENT, SessionDeviceMove.hiddenReason(DomainContract.sessionKindAgent, false))
        assertEquals(SessionDeviceMove.REASON_ASIDE, SessionDeviceMove.hiddenReason(DomainContract.sessionKindThread, true))
        assertTrue(SessionDeviceMove.visible(DomainContract.sessionKindThread, false))
    }

    @Test
    fun blockedOrderIsSteerThenEnded() {
        assertEquals(SessionDeviceMove.REASON_STEER, SessionDeviceMove.blockedReason(canSteer = false, ended = true))
        assertEquals(SessionDeviceMove.REASON_ENDED, SessionDeviceMove.blockedReason(canSteer = true, ended = true))
        assertNull(SessionDeviceMove.blockedReason(canSteer = true, ended = false))
        assertTrue(SessionDeviceMove.canSteer(shared = true, starterId = "a", currentUserId = "b"))
        assertFalse(SessionDeviceMove.canSteer(shared = false, starterId = "a", currentUserId = "b"))
        assertTrue(SessionDeviceMove.canSteer(shared = false, starterId = "a", currentUserId = "a"))
    }

    @Test
    fun targetReasonsInOrder() {
        // Offline wins over every other reason.
        assertEquals(
            SessionDeviceMove.REASON_OFFLINE,
            SessionDeviceMove.targetReason(device("x", online = false, shared = true, agents = emptyList()), team, false, "claude"),
        )
        assertEquals(
            SessionDeviceMove.REASON_VISIBILITY,
            SessionDeviceMove.targetReason(device("x", shared = true, agents = emptyList()), team, false, "claude"),
        )
        // Update (a build before MOVE_MIN_VERSION) comes before no-login.
        assertEquals(
            SessionDeviceMove.REASON_UPDATE,
            SessionDeviceMove.targetReason(device("x", agents = emptyList(), version = "0.14.72"), team, false, "claude"),
        )
        assertEquals(
            SessionDeviceMove.REASON_UPDATE,
            SessionDeviceMove.targetReason(device("x", version = null), team, false, "claude"),
        )
        assertEquals(
            SessionDeviceMove.REASON_NO_LOGIN,
            SessionDeviceMove.targetReason(device("x", agents = listOf("codex")), team, false, "claude"),
        )
        assertNull(SessionDeviceMove.targetReason(device("x", shared = true), team, true, null))
    }

    // Web `deviceCanReceiveSession`: unreported or unparsable = old; a
    // `-staging`/`+build` suffix is ignored; missing segments read as 0.
    @Test
    fun canReceiveSessionMatchesWebVersionRules() {
        assertEquals(listOf(0, 14, 73), SessionDeviceMove.MOVE_MIN_VERSION)
        assertFalse(SessionDeviceMove.canReceiveSession(null))
        assertFalse(SessionDeviceMove.canReceiveSession(""))
        assertFalse(SessionDeviceMove.canReceiveSession("dev"))
        assertFalse(SessionDeviceMove.canReceiveSession("0.14.72"))
        assertFalse(SessionDeviceMove.canReceiveSession("0.14"))
        assertTrue(SessionDeviceMove.canReceiveSession("0.14.73"))
        assertTrue(SessionDeviceMove.canReceiveSession("0.14.74-staging"))
        assertTrue(SessionDeviceMove.canReceiveSession("0.15.0+abc123"))
        assertTrue(SessionDeviceMove.canReceiveSession("1.0"))
    }

    @Test
    fun rowsMarkCurrentAndTarget() {
        val rows = SessionDeviceMove.rows(
            listOf(device("a"), device("b"), device("c", online = false)),
            deviceId = "a",
            moveToDeviceId = "b",
            teamId = team,
            shared = false,
            cli = "claude",
        )
        assertEquals(listOf("a", "b", "c"), rows.map { it.id })
        assertTrue(rows[0].current)
        assertNull(rows[0].reason)
        assertTrue(rows[1].target)
        assertEquals(SessionDeviceMove.REASON_OFFLINE, rows[2].reason)
        assertEquals("Moving to B…", SessionDeviceMove.pinLabel(rows, "a", "b"))
        assertEquals("A", SessionDeviceMove.pinLabel(rows, "a", null))
    }

    @Test
    fun currentDeviceAlwaysListed() {
        val rows = SessionDeviceMove.rows(listOf(device("b")), "gone", null, team, false, "claude")
        assertEquals(listOf("gone", "b"), rows.map { it.id })
        assertTrue(rows.first().current)
    }

    @Test
    fun ownerSuffixMatchesLaunchName() {
        val shared = device("srv", shared = true, owner = DeviceOwner(id = "u2", name = "Dennis"))
        assertEquals("SRV — Dennis", SessionDeviceMove.name(shared))
    }

    @Test
    fun picksMoveCancelOrNothing() {
        assertEquals(DeviceMovePick.Move, SessionDeviceMove.pick("b", "a", null))
        assertNull(SessionDeviceMove.pick("a", "a", null))
        assertEquals(DeviceMovePick.Cancel, SessionDeviceMove.pick("a", "a", "b"))
        assertNull(SessionDeviceMove.pick("b", "a", "b"))
        assertEquals(DeviceMovePick.Move, SessionDeviceMove.pick("c", "a", "b"))
    }

    @Test
    fun wireIsSessionIdPlusDeviceId() {
        val input = SessionsSendInput.moveToDevice("s1", "b")
        assertEquals("s1", input.sessionId)
        assertEquals("b", input.deviceId)
        assertNull(input.text)
        assertNull(input.account)
    }
}
