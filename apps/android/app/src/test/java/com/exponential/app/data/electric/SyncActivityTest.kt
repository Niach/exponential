package com.exponential.app.data.electric

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Truth table for the "Syncing…" chip (EXP-264). The rule it encodes: say
 * "syncing" while a core shape is demonstrably behind, and shut up otherwise —
 * an indicator that never turns off is noise, not information.
 */
class SyncActivityTest {

    private val now = 100_000L

    private fun shapes(
        vararg overrides: Pair<String, SyncStats.ShapeStatus>,
    ): Map<String, SyncStats.ShapeStatus> {
        val live = CORE_SHAPES.associateWith { shape ->
            SyncStats.ShapeStatus(shape = shape, phase = "live", lastSuccessAtMs = now)
        }
        return live + overrides
    }

    @Test
    fun coreShapesAreTheIssueListShapesOnly() {
        assertEquals(
            setOf("teams", "boards", "issues", "issue_labels", "labels"),
            CORE_SHAPES,
        )
    }

    @Test
    fun issueStatusesIsNotCoreSoAnOldServerCannotWedgeTheGate() {
        // A pre-EXP-314 server 404s the issue_statuses shape forever. If it
        // were core, its never-completing initial snapshot would pin
        // isCatchingUp (and the refresh wait) to true for the whole session —
        // while the list itself renders fine off the constructed builtins.
        val map = shapes() + ("issue_statuses" to SyncStats.ShapeStatus("issue_statuses", phase = "initial"))
        assertFalse(isCatchingUp(map, lastKickAt = now - 200, now = now))
    }

    @Test
    fun healthyLiveShapesAreNotCatchingUp() {
        assertFalse(isCatchingUp(shapes(), lastKickAt = now - 200, now = now))
    }

    @Test
    fun anInitialSnapshotCountsAsCatchingUp() {
        val map = shapes("issues" to SyncStats.ShapeStatus("issues", phase = "initial"))
        // True even with no kick at all — a first sync is exactly the case the
        // user most needs told about.
        assertTrue(isCatchingUp(map, lastKickAt = 0L, now = now))
    }

    @Test
    fun aCatchupPhaseCountsAsCatchingUp() {
        val map = shapes("labels" to SyncStats.ShapeStatus("labels", phase = "catchup"))
        assertTrue(isCatchingUp(map, lastKickAt = 0L, now = now))
    }

    @Test
    fun nonCoreShapesAreIgnored() {
        val map = shapes() + ("comments" to SyncStats.ShapeStatus("comments", phase = "initial"))
        assertFalse(isCatchingUp(map, lastKickAt = now - 200, now = now))
    }

    @Test
    fun withoutAKickThereIsNothingToBeBehindOn() {
        val map = shapes("issues" to SyncStats.ShapeStatus("issues", phase = "live", lastSuccessAtMs = 0L))
        assertFalse(isCatchingUp(map, lastKickAt = 0L, now = now))
    }

    @Test
    fun aShapeThatHasNotPolledSinceTheKickIsCatchingUp() {
        val kickAt = now - 1_000
        val map = shapes(
            "issues" to SyncStats.ShapeStatus("issues", phase = "live", lastSuccessAtMs = kickAt - 10_000),
        )
        assertTrue(isCatchingUp(map, lastKickAt = kickAt, now = now))
    }

    @Test
    fun aShapeMissingEntirelyCountsAsBehindAfterAKick() {
        val kickAt = now - 1_000
        val map = shapes().filterKeys { it != "boards" }
        assertTrue(isCatchingUp(map, lastKickAt = kickAt, now = now))
        assertTrue("a null map is behind too", isCatchingUp(null, lastKickAt = kickAt, now = now))
    }

    @Test
    fun aFreshnessSuppressedKickIsAlreadySatisfied() {
        // The shape polled just BEFORE the kick, which is why the kick was
        // dropped as redundant — that must read as caught up, not as behind
        // forever.
        val kickAt = now - 1_000
        val map = shapes(
            "issues" to SyncStats.ShapeStatus(
                "issues",
                phase = "live",
                lastSuccessAtMs = kickAt - KICK_FRESHNESS_MS + 1,
            ),
        )
        assertFalse(isCatchingUp(map, lastKickAt = kickAt, now = now))
    }

    @Test
    fun anOldKickStopsCountingSoOfflineDevicesDoNotSpinForever() {
        val kickAt = now - 20_000
        val map = shapes("issues" to SyncStats.ShapeStatus("issues", phase = "live", lastSuccessAtMs = 0L))
        assertFalse(isCatchingUp(map, lastKickAt = kickAt, now = now))
        // …but inside the window the same state does show.
        assertTrue(isCatchingUp(map, lastKickAt = now - 5_000, now = now))
    }
}
