package com.exponential.app.ui.gettingstarted

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// Mirrors web's `getting-started-model.test.ts` for the seven entries mobile
// shows — same order, same owner gating, same lock chain (EXP-698 r5).
class GettingStartedModelTest {

    private fun keys(entries: List<GettingStartedEntry>) = entries.map { it.key }

    private fun stateOf(entries: List<GettingStartedEntry>, key: GettingStartedEntryKey) =
        entries.first { it.key == key }

    @Test
    fun aFreshOwnerSeesSevenStepsInOrderAndNoneDone() {
        val entries = deriveGettingStartedEntries(GettingStartedSignals(), isOwner = true)
        assertEquals(
            listOf(
                GettingStartedEntryKey.Desktop,
                GettingStartedEntryKey.Github,
                GettingStartedEntryKey.Invite,
                GettingStartedEntryKey.Board,
                GettingStartedEntryKey.Coding,
                GettingStartedEntryKey.Action,
                GettingStartedEntryKey.Server,
            ),
            keys(entries),
        )
        assertEquals(0, GettingStartedState(entries, loading = false).done)
    }

    @Test
    fun aMemberNeitherSeesNorCountsTheOwnerOnlySteps() {
        val entries = deriveGettingStartedEntries(GettingStartedSignals(), isOwner = false)
        assertEquals(
            listOf(
                GettingStartedEntryKey.Desktop,
                GettingStartedEntryKey.Github,
                GettingStartedEntryKey.Board,
                GettingStartedEntryKey.Coding,
                GettingStartedEntryKey.Server,
            ),
            keys(entries),
        )
        assertEquals(5, GettingStartedState(entries, loading = false).total)
    }

    @Test
    fun codingLocksOnTheMissingFeederInDisplayOrder() {
        // No machine at all: the desktop step is what to do first, even
        // though GitHub is missing too.
        val noMachine = deriveGettingStartedEntries(
            GettingStartedSignals(githubInstalled = false, hasBoard = true),
            isOwner = true,
        )
        assertEquals(
            GettingStartedEntryState.Locked,
            stateOf(noMachine, GettingStartedEntryKey.Coding).state,
        )
        assertEquals(
            GettingStartedEntryKey.Desktop,
            stateOf(noMachine, GettingStartedEntryKey.Coding).lockedBy,
        )

        // A machine but no GitHub: GitHub is the feeder, since the board step
        // cannot attach a repo without it.
        val noGithub = deriveGettingStartedEntries(
            GettingStartedSignals(hasDesktopDevice = true, hasBoard = true),
            isOwner = true,
        )
        assertEquals(
            GettingStartedEntryKey.Github,
            stateOf(noGithub, GettingStartedEntryKey.Coding).lockedBy,
        )

        // GitHub connected but no repo-backed board: the board step is next.
        val noRepoBoard = deriveGettingStartedEntries(
            GettingStartedSignals(
                hasDesktopDevice = true,
                githubInstalled = true,
                hasBoard = true,
            ),
            isOwner = true,
        )
        assertEquals(
            GettingStartedEntryKey.Board,
            stateOf(noRepoBoard, GettingStartedEntryKey.Coding).lockedBy,
        )
    }

    @Test
    fun aServerAloneUnlocksCodingAndActions() {
        // A registered CLI daemon is a machine — the desktop app is not the
        // only way to run something.
        val entries = deriveGettingStartedEntries(
            GettingStartedSignals(
                hasServerDevice = true,
                githubInstalled = true,
                hasBoard = true,
                hasRepoBoard = true,
            ),
            isOwner = true,
        )
        assertEquals(
            GettingStartedEntryState.Available,
            stateOf(entries, GettingStartedEntryKey.Coding).state,
        )
        assertEquals(
            GettingStartedEntryState.Available,
            stateOf(entries, GettingStartedEntryKey.Action).state,
        )
        assertEquals(
            GettingStartedEntryState.Done,
            stateOf(entries, GettingStartedEntryKey.Server).state,
        )
        assertNull(stateOf(entries, GettingStartedEntryKey.Coding).lockedBy)
    }

    @Test
    fun completionWinsOverLocking() {
        // A coding session exists even though nothing else does — the signal
        // proves the prerequisites were satisfiable at some point.
        val entries = deriveGettingStartedEntries(
            GettingStartedSignals(hasCodingSession = true, hasAction = true),
            isOwner = true,
        )
        assertEquals(
            GettingStartedEntryState.Done,
            stateOf(entries, GettingStartedEntryKey.Coding).state,
        )
        assertEquals(
            GettingStartedEntryState.Done,
            stateOf(entries, GettingStartedEntryKey.Action).state,
        )
    }

    @Test
    fun theChecklistDisappearsOnlyWhenEverySeenStepIsDone() {
        val signals = GettingStartedSignals(
            hasDesktopDevice = true,
            hasServerDevice = true,
            githubInstalled = true,
            hasInvitedTeam = true,
            hasBoard = true,
            hasRepoBoard = true,
            hasCodingSession = true,
            hasAction = true,
        )
        val complete = GettingStartedState(
            deriveGettingStartedEntries(signals, isOwner = true),
            loading = false,
        )
        assertEquals(complete.total, complete.done)
        assertTrue(complete.complete)

        // Still loading is incomplete-unknown, never complete.
        assertFalse(complete.copy(loading = true).complete)
        // And an empty (unresolved) checklist is not "all done".
        assertFalse(GettingStartedState(emptyList(), loading = false).complete)
    }
}
