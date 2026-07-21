package com.exponential.app.data.api

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// Compliance lock (EXP-216 / store billing policy): server plan-cap messages
// carry purchase language ("Add seats or upgrade…") that must never render in
// the app — trpcUserMessageFromBody swaps them for the neutral copy. Ordinary
// errors keep passing the server message through verbatim. Since EXP-219 this
// sanitization runs at TrpcClient's throw site, so every TrpcException message
// is user-presentable and trpcErrorMessage only supplies the non-tRPC fallback.
class TrpcErrorMessageTest {
    private fun trpcBody(message: String, code: String = "PRECONDITION_FAILED") =
        """{"error": {"data": {"code": "$code"}, "message": "$message"}}"""

    @Test
    fun planLimitMessageIsNeutralized() {
        val body = trpcBody("Your plan allows up to 1 seat. Add seats or upgrade to invite more teammates.")
        assertEquals(PLAN_LIMIT_NEUTRAL_MESSAGE, trpcUserMessageFromBody(body))
    }

    @Test
    fun planLimitMessageIsNeutralizedInNestedJsonEnvelope() {
        val body = """{"error": {"json": {"message": "Your plan allows up to 10 teams on the free plan. Upgrade to create more."}}}"""
        assertEquals(PLAN_LIMIT_NEUTRAL_MESSAGE, trpcUserMessageFromBody(body))
    }

    @Test
    fun ordinaryPreconditionFailedPassesThroughVerbatim() {
        val body = trpcBody("No repository linked to this board")
        assertEquals("No repository linked to this board", trpcUserMessageFromBody(body))
    }

    @Test
    fun unparsableBodyYieldsNull() {
        assertNull(trpcUserMessageFromBody("<html>bad gateway</html>"))
        assertNull(trpcUserMessageFromBody("""{"unrelated": true}"""))
        assertNull(trpcUserMessageFromBody(""))
    }

    @Test
    fun trpcExceptionMessagePassesThroughVerbatim() {
        val error = TrpcException("No repository linked to this board")
        assertEquals("No repository linked to this board", trpcErrorMessage(error, "fallback"))
    }

    @Test
    fun nonTrpcExceptionFallsBackToFallback() {
        assertEquals("fallback", trpcErrorMessage(RuntimeException("boom"), "fallback"))
    }
}
