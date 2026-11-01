package com.exponential.app.domain

import com.exponential.app.data.api.DeviceOwner
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.api.SteerDevice
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
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
        version: String? = null,
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
        assertEquals("Couldn't move the session", SessionDeviceMove.MOVE_FAILED_TOAST)
        assertEquals("This device is offline.", SessionDeviceMove.REASON_OFFLINE)
        assertEquals("Agents move from Settings → Agents.", SessionDeviceMove.REASON_AGENT)
    }

    /** #EXP-1440: personal → usable, shared with the team → usable, shared
     *  only with another team → not usable. */
    @Test
    fun deviceUsableInTeamUnlessAnotherTeamOwnsIt() {
        assertTrue(SessionDeviceMove.deviceUsableInTeam(device("a"), team))
        assertTrue(SessionDeviceMove.deviceUsableInTeam(device("b", shared = true), team))
        assertFalse(SessionDeviceMove.deviceUsableInTeam(device("c").copy(sharedTeamIds = listOf("team-2")), team))
        assertEquals("This device belongs to another team", SessionDeviceMove.DEVICE_OTHER_TEAM_MESSAGE)
    }

    @Test
    fun pinHiddenForAgents() {
        assertEquals(SessionDeviceMove.REASON_AGENT, SessionDeviceMove.hiddenReason(DomainContract.sessionKindAgent))
        assertNull(SessionDeviceMove.hiddenReason(DomainContract.sessionKindThread))
        assertTrue(SessionDeviceMove.visible(DomainContract.sessionKindThread))
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
        // No version gate: an old or unreported build is never refused for it
        // (the CLIENT_MIN_VERSION floor does that job).
        assertNull(SessionDeviceMove.targetReason(device("x", version = "0.14.72"), team, false, "claude"))
        assertNull(SessionDeviceMove.targetReason(device("x", version = null), team, false, "claude"))
        assertEquals(
            SessionDeviceMove.REASON_NO_LOGIN,
            SessionDeviceMove.targetReason(device("x", agents = listOf("codex")), team, false, "claude"),
        )
        assertNull(SessionDeviceMove.targetReason(device("x", shared = true), team, true, null))
    }

    @Test
    fun rowsMarkCurrent() {
        val rows = SessionDeviceMove.rows(
            listOf(device("a"), device("b"), device("c", online = false)),
            deviceId = "a",
            teamId = team,
            shared = false,
            cli = "claude",
        )
        assertEquals(listOf("a", "b", "c"), rows.map { it.id })
        assertTrue(rows[0].current)
        assertNull(rows[0].reason)
        assertFalse(rows[1].current)
        assertNull(rows[1].reason)
        assertEquals(SessionDeviceMove.REASON_OFFLINE, rows[2].reason)
        assertEquals("A", SessionDeviceMove.pinLabel(rows, "a"))
    }

    @Test
    fun currentDeviceAlwaysListed() {
        val rows = SessionDeviceMove.rows(listOf(device("b")), "gone", team, false, "claude")
        assertEquals(listOf("gone", "b"), rows.map { it.id })
        assertTrue(rows.first().current)
    }

    @Test
    fun ownerSuffixMatchesLaunchName() {
        val shared = device("srv", shared = true, owner = DeviceOwner(id = "u2", name = "Dennis"))
        assertEquals("SRV — Dennis", SessionDeviceMove.name(shared))
    }

    @Test
    fun picksMoveOrNothing() {
        assertEquals(DeviceMovePick.Move, SessionDeviceMove.pick("b", "a"))
        assertNull(SessionDeviceMove.pick("a", "a"))
    }

    @Test
    fun wireIsSessionIdPlusDeviceId() {
        val input = SessionsSendInput.moveToDevice("s1", "b")
        assertEquals("s1", input.sessionId)
        assertEquals("b", input.deviceId)
        assertNull(input.text)
        assertNull(input.account)
    }

    // #EXP-1541: the pickers ×4 replay fixtures/session-device-move.json.

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("session-device-move.json")).jsonObject
    }

    private fun JsonObject.flag(key: String) = getValue(key).jsonPrimitive.boolean
    private fun JsonObject.text(key: String) = this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    @Test
    fun fixtureCopyMatches() {
        val reasons = fixture.getValue("reasons").jsonObject
        assertEquals(SessionDeviceMove.REASON_OFFLINE, reasons.text("offline"))
        assertEquals(SessionDeviceMove.REASON_VISIBILITY, reasons.text("visibility"))
        assertEquals(SessionDeviceMove.REASON_NO_LOGIN, reasons.text("noLogin"))
    }

    @Test
    fun replaysVisibilityCases() {
        val cases = fixture.getValue("visibilityCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val expected = case["expected"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.boolean
            assertEquals(
                case.text("name"),
                expected,
                SessionDeviceMove.visibilityMove(
                    sessionShared = input.flag("sessionShared"),
                    targetShared = input.flag("targetShared"),
                    queuedRoot = input.flag("queuedRoot"),
                    targetIsStarters = input.flag("targetIsStarters"),
                    moverIsStarter = input.flag("moverIsStarter"),
                ),
            )
        }
    }

    @Test
    fun replaysQueuedRootCases() {
        val cases = fixture.getValue("queuedRootCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(
                case.toString(),
                case.flag("expected"),
                SessionDeviceMove.queuedRoot(case.text("status")!!, case.text("parentId"), case.flag("hasSubSessions")),
            )
        }
    }

    @Test
    fun replaysTargetCases() {
        val reasons = fixture.getValue("reasons").jsonObject
        val cases = fixture.getValue("targetCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val target = case.getValue("device").jsonObject
            val session = case.getValue("session").jsonObject
            val mover = case.text("mover")!!
            // `deviceOwner: null` / the mover's own row = a row without `owner`.
            val ownerId = target.text("deviceOwner")
            val reason = SessionDeviceMove.targetReason(
                device(
                    "target",
                    online = target.flag("online"),
                    shared = target.flag("targetShared"),
                    agents = if (target.flag("signedIn")) listOf("claude") else listOf("codex"),
                    owner = if (ownerId == null || ownerId == mover) null else DeviceOwner(id = ownerId, name = "Mate"),
                ),
                teamId = team,
                shared = session.flag("shared"),
                cli = "claude",
                queuedRoot = session.flag("queuedRoot"),
                starterId = session.text("starter"),
                viewerId = mover,
            )
            assertEquals(case.text("name"), case.text("expected")?.let { reasons.text(it) }, reason)
        }
    }
}
