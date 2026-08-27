package com.exponential.app.domain

import com.exponential.app.data.api.DeviceOwner
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.CodingSessionEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-637: the four labels a finished run can carry are byte-equal across web,
 * desktop, iOS and Android, and Resume is offered only where the server would
 * actually accept it.
 */
class RunOutcomeTest {

    private fun session(
        id: String = "sess-1",
        userId: String = "me",
        status: String = DomainContract.codingSessionStatusEnded,
        deviceId: String? = "dev-1",
    ) = CodingSessionEntity(
        id = id,
        teamId = "team-1",
        userId = userId,
        deviceId = deviceId,
        status = status,
        startedAt = "2026-08-11T10:00:00Z",
        createdAt = "2026-08-11T10:00:00Z",
        updatedAt = "2026-08-11T10:20:00Z",
    )

    private fun device(
        deviceId: String = "dev-1",
        label: String = "buildbox",
        online: Boolean = true,
        caps: List<String>? = listOf("actions", "resume-run"),
        owner: DeviceOwner? = null,
    ) = SteerDevice(
        deviceId = deviceId,
        deviceLabel = label,
        online = online,
        caps = caps,
        owner = owner,
    )

    // ── Labels (byte-equal on every client) ─────────────────────────────────

    @Test
    fun `every contract outcome maps to its shared label`() {
        assertEquals("Done", runOutcomeLabel(DomainContract.codingSessionOutcomeDone))
        assertEquals("Blocked", runOutcomeLabel(DomainContract.codingSessionOutcomeBlocked))
        assertEquals("No changes", runOutcomeLabel(DomainContract.codingSessionOutcomeNoChanges))
    }

    @Test
    fun `no outcome reads Ended, and so does an unknown one`() {
        // A run killed, merged or swept never gets an outcome — and neither did
        // anything that ended before EXP-637.
        assertEquals("Ended", runOutcomeLabel(null))
        assertEquals(RunOutcome.Ended, runOutcomeOf(null))
        // A newer server's value must never leak its raw wire token into the UI.
        assertEquals("Ended", runOutcomeLabel("some_future_outcome"))
    }

    @Test
    fun `the contract's outcome values are exactly the three presentable ones`() {
        assertEquals(
            listOf(RunOutcome.Done, RunOutcome.Blocked, RunOutcome.NoChanges),
            DomainContract.codingSessionOutcomeValues.map(::runOutcomeOf),
        )
    }

    // ── Resume eligibility ──────────────────────────────────────────────────

    @Test
    fun `an own ended run on its capable online machine resumes`() {
        val target = resumeTargetFor(session(), listOf(device()), "me")
        assertNotNull(target)
        assertEquals("sess-1", target!!.sessionId)
        assertEquals("dev-1", target.deviceId)
        assertEquals("buildbox", target.deviceLabel)
    }

    @Test
    fun `a live run is steered, not resumed`() {
        for (status in CodingSessionLiveness.liveStatuses) {
            assertNull(resumeTargetFor(session(status = status), listOf(device()), "me"))
        }
    }

    @Test
    fun `a teammate's run and a signed-out reader never resume`() {
        assertNull(resumeTargetFor(session(userId = "someone-else"), listOf(device()), "me"))
        assertNull(resumeTargetFor(session(), listOf(device()), null))
    }

    @Test
    fun `the run's own machine must be reachable and capable`() {
        // No stamped device: nothing holds the worktree.
        assertNull(resumeTargetFor(session(deviceId = null), listOf(device()), "me"))
        // Another machine cannot take it — only the one that ran it.
        assertNull(resumeTargetFor(session(), listOf(device(deviceId = "dev-2")), "me"))
        // Offline.
        assertNull(resumeTargetFor(session(), listOf(device(online = false)), "me"))
        // Too old to know how (and a machine advertising nothing at all).
        assertNull(resumeTargetFor(session(), listOf(device(caps = listOf("actions"))), "me"))
        assertNull(resumeTargetFor(session(), listOf(device(caps = null)), "me"))
        // The machine isn't synced at all.
        assertNull(resumeTargetFor(session(), emptyList(), "me"))
    }

    @Test
    fun `a run on a teammate's shared server resumes there`() {
        // EXP-432: the run is still the caller's; the machine that holds its
        // worktree is the one it has to go back to.
        val shared = device(owner = DeviceOwner(id = "someone-else", name = "Ada"))
        assertEquals("dev-1", resumeTargetFor(session(), listOf(shared), "me")?.deviceId)
    }

    @Test
    fun `the caller's own row wins when the same machine id appears twice`() {
        val shared = device(label = "theirs", owner = DeviceOwner(id = "other", name = "Ada"))
        val own = device(label = "mine")
        assertEquals("mine", resumeTargetFor(session(), listOf(shared, own), "me")?.deviceLabel)
    }

    @Test
    fun `an unlabelled machine still reads as something`() {
        val target = resumeTargetFor(session(), listOf(device(label = "")), "me")
        assertEquals("dev-1", target?.deviceLabel)
    }
}
