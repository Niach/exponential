package com.exponential.app.data.api

import org.junit.Assert.assertEquals
import org.junit.Test

// Compliance lock (EXP-216 / store billing policy): server plan-cap messages
// carry purchase language ("Add seats or upgrade…") that must never render in
// the app — trpcErrorMessage swaps them for the neutral copy. Ordinary errors
// keep passing the server message through verbatim.
class TrpcErrorMessageTest {
    private fun trpcBody(message: String, code: String = "PRECONDITION_FAILED") =
        """{"error": {"data": {"code": "$code"}, "message": "$message"}}"""

    @Test
    fun planLimitMessageIsNeutralized() {
        val error = TrpcException(
            "tRPC teamInvites.create HTTP 412: " +
                trpcBody("Your plan allows up to 1 seat. Add seats or upgrade to invite more teammates."),
        )
        val message = trpcErrorMessage(error, "fallback")
        assertEquals(PLAN_LIMIT_NEUTRAL_MESSAGE, message)
    }

    @Test
    fun planLimitMessageIsNeutralizedInNestedJsonEnvelope() {
        val error = TrpcException(
            """tRPC teams.create HTTP 412: {"error": {"json": {"message": "Your plan allows up to 10 teams on the free plan. Upgrade to create more."}}}""",
        )
        assertEquals(PLAN_LIMIT_NEUTRAL_MESSAGE, trpcErrorMessage(error, "fallback"))
    }

    @Test
    fun ordinaryPreconditionFailedPassesThroughVerbatim() {
        val error = TrpcException(
            "tRPC codingSessions.start HTTP 412: " + trpcBody("No repository linked to this board"),
        )
        assertEquals("No repository linked to this board", trpcErrorMessage(error, "fallback"))
    }

    @Test
    fun unparsableBodyFallsBackToFallback() {
        val error = TrpcException("tRPC HTTP 502: <html>bad gateway</html>")
        assertEquals("fallback", trpcErrorMessage(error, "fallback"))
    }

    @Test
    fun nonTrpcExceptionFallsBackToFallback() {
        assertEquals("fallback", trpcErrorMessage(RuntimeException("boom"), "fallback"))
    }
}
