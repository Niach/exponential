package com.exponential.app.domain

import com.exponential.app.data.api.OFFLINE_MESSAGE
import com.exponential.app.data.api.TrpcException
import io.ktor.http.HttpStatusCode
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1391 + EXP-1378: a refused merge toasts the server's message; ONLY the
 * check gate's PRECONDITION_REQUIRED (HTTP 428) swaps Retry for `Merge
 * anyway`. A content conflict is no longer special (the Fix conflicts run is
 * gone; the server hands the refusal to the thread).
 */
class MergeFailureTest {

    private val fallback = "The pull request could not be merged"

    @Test
    fun theCheckGateOffersMergeAnyway() {
        val failure = MergeFailure.from(
            TrpcException(
                "Checks are still running on PR #45: \"gate\".",
                HttpStatusCode(428, "Precondition Required"),
                code = "PRECONDITION_REQUIRED",
            ),
            fallback,
        )
        assertEquals("Checks are still running on PR #45: \"gate\".", failure.message)
        assertTrue(failure.isChecks)
        // The code alone decides, and so does the status alone.
        assertTrue(MergeFailure.from(TrpcException("x", code = "PRECONDITION_REQUIRED"), fallback).isChecks)
        assertTrue(MergeFailure.from(TrpcException("x", HttpStatusCode(428, "")), fallback).isChecks)
    }

    // A conflict is an ordinary refusal now: Retry, no recovery run.
    @Test
    fun aConflictIsAnOrdinaryRefusal() {
        val failure = MergeFailure.from(
            TrpcException("This branch has conflicts that must be resolved", HttpStatusCode.Conflict, code = "CONFLICT"),
            fallback,
        )
        assertEquals("This branch has conflicts that must be resolved", failure.message)
        assertFalse(failure.isChecks)
    }

    @Test
    fun otherServerRefusalsAreNotTheCheckGate() {
        val failure = MergeFailure.from(
            TrpcException(
                "Squash merging is not allowed on this repository",
                HttpStatusCode.PreconditionFailed,
                code = "PRECONDITION_FAILED",
            ),
            fallback,
        )
        assertEquals("Squash merging is not allowed on this repository", failure.message)
        assertFalse(failure.isChecks)
    }

    @Test
    fun offlineMergeReadsAsOffline() {
        val failure = MergeFailure.from(java.net.UnknownHostException("no dns"), fallback)
        assertEquals(OFFLINE_MESSAGE, failure.message)
        assertFalse(failure.isChecks)
    }

    @Test
    fun unrecognizedFailureFallsBackToTheCaptionCopy() {
        val failure = MergeFailure.from(IllegalStateException("boom"), fallback)
        assertEquals(fallback, failure.message)
        assertFalse(failure.isChecks)
    }

    // EXP-1145: a stack merge shows the server's message; the check gate
    // still offers Merge anyway (web `MergeThroughConfirm` parity).
    @Test
    fun aFailedStackMergeShowsTheMessage() {
        val failure = MergeFailure.fromStack(
            TrpcException("EXP-2 has conflicts that must be resolved", HttpStatusCode.Conflict),
        )
        assertEquals("EXP-2 has conflicts that must be resolved", failure.message)
        assertFalse(failure.isChecks)
        assertTrue(MergeFailure.fromStack(TrpcException("red", code = "PRECONDITION_REQUIRED")).isChecks)
        assertEquals(
            "The stack could not be merged",
            MergeFailure.fromStack(IllegalStateException("boom")).message,
        )
    }

}
