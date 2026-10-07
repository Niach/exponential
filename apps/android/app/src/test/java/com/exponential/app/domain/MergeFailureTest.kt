package com.exponential.app.domain

import com.exponential.app.data.api.OFFLINE_MESSAGE
import com.exponential.app.data.api.TrpcException
import io.ktor.http.HttpStatusCode
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-533: the "Fix merge conflicts" offer. The bug this locks is the one the
 * issue reported — a merge attempted with no connection at all still showed
 * the recovery button, sending the user to start an agent run against a
 * problem that isn't a conflict. EXP-1233: the same gate now decides whether
 * a refused merge OPENS the Fix merge conflicts composer at once.
 */
class MergeFailureTest {

    private val fallback = "The pull request could not be merged"

    @Test
    fun realConflictCarriesTheServerMessageAndOffersTheRun() {
        val failure = MergeFailure.from(
            TrpcException("This branch has conflicts that must be resolved", HttpStatusCode.Conflict),
            fallback,
        )
        assertEquals("This branch has conflicts that must be resolved", failure.message)
        assertTrue(failure.isConflict)
        assertTrue(canOfferFixConflicts(failure, "exp/EXP-533"))
    }

    @Test
    fun offlineMergeReadsAsOfflineAndOffersNothing() {
        val failure = MergeFailure.from(java.net.UnknownHostException("no dns"), fallback)
        assertEquals(OFFLINE_MESSAGE, failure.message)
        assertFalse(failure.isConflict)
        assertFalse(canOfferFixConflicts(failure, "exp/EXP-533"))
    }

    // Branch protection, a stale base, an unconfigured GitHub App: all real
    // server answers, none of them fixable by a rebase.
    @Test
    fun otherServerRefusalsShowTheReasonWithoutTheRun() {
        val failure = MergeFailure.from(
            TrpcException("Squash merging is not allowed on this repository", HttpStatusCode.PreconditionFailed),
            fallback,
        )
        assertEquals("Squash merging is not allowed on this repository", failure.message)
        assertFalse(failure.isConflict)
        assertFalse(canOfferFixConflicts(failure, "exp/EXP-533"))
    }

    @Test
    fun unrecognizedFailureFallsBackToTheCaptionCopy() {
        val failure = MergeFailure.from(IllegalStateException("boom"), fallback)
        assertEquals(fallback, failure.message)
        assertFalse(failure.isConflict)
    }

    // The run rebases the PR's branch and force-pushes it, so a row with no
    // recorded branch has nothing for it to act on.
    @Test
    fun aConflictWithoutABranchOffersNothing() {
        val failure = MergeFailure("conflicts", isConflict = true)
        assertFalse(canOfferFixConflicts(failure, null))
        assertFalse(canOfferFixConflicts(failure, ""))
        assertFalse(canOfferFixConflicts(failure, "   "))
    }

    // Surfaces with their own remote-start gate (the issue Changes bar, the
    // Agents rows) pass it in; nothing to run on means nothing to offer.
    @Test
    fun steerMustBeAvailable() {
        val failure = MergeFailure("conflicts", isConflict = true)
        assertFalse(canOfferFixConflicts(failure, "exp/EXP-533", steerEnabled = false))
        assertTrue(canOfferFixConflicts(failure, "exp/EXP-533", steerEnabled = true))
    }

    // EXP-1145: a stack merge lands several pull requests, and the conflict
    // may sit in another member: the message shows, the run is never offered.
    @Test
    fun aFailedStackMergeShowsTheMessageAndNeverOffersTheRun() {
        val failure = MergeFailure.fromStack(
            TrpcException("EXP-2 has conflicts that must be resolved", HttpStatusCode.Conflict),
        )
        assertEquals("EXP-2 has conflicts that must be resolved", failure.message)
        assertFalse(failure.isConflict)
        assertFalse(canOfferFixConflicts(failure, "exp/EXP-1"))
        assertEquals(
            "The stack could not be merged",
            MergeFailure.fromStack(IllegalStateException("boom")).message,
        )
    }

    @Test
    fun noFailureOffersNothing() {
        assertFalse(canOfferFixConflicts(null, "exp/EXP-533"))
    }
}
