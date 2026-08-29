package com.exponential.app.data.api

import io.ktor.http.HttpStatusCode
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// Compliance lock (EXP-216 / store billing policy): server plan-cap messages
// carry purchase language ("Add seats or upgrade…") that must never render in
// the app — trpcUserMessageFromBody swaps them for the neutral copy. Ordinary
// errors keep passing the server message through verbatim. Since EXP-219 this
// sanitization runs at TrpcClient's throw site, so every TrpcException message
// is user-presentable and trpcErrorMessage only supplies the non-tRPC fallback.
// The same allowlist rewrites the REV2-55 team-delete billing gate, whose
// server copy points at a web-only Billing screen.
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

    // REV2-55: deleting a team is refused while its subscription is live. The
    // server points at "team settings → Billing"; billing is web-only here, so
    // the app substitutes copy that names the web.
    @Test
    fun teamDeleteSubscriptionGateIsRewrittenForNative() {
        val body = trpcBody(
            "This team has an active subscription. Cancel the subscription in team settings → Billing before deleting the team."
        )
        val message = trpcUserMessageFromBody(body)
        assertEquals(TEAM_DELETE_SUBSCRIPTION_MESSAGE, message)
        assertFalse(message!!.contains("team settings"))
    }

    @Test
    fun otherSubscriptionMessagesPassThroughVerbatim() {
        // billing.cancelSubscription's "has NO active subscription" is a
        // different precondition — the prefix must not swallow it.
        val body = trpcBody("This team has no active subscription")
        assertEquals("This team has no active subscription", trpcUserMessageFromBody(body))
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

    // EXP-533: the issue this suite's offline half exists for — the New Issue
    // screen rendered okhttp's `Unable to resolve host "app.exponential.at":
    // No address associated with hostname`. A request that never reached a
    // server has nothing server-shaped to say; it says the user is offline.
    @Test
    fun transportFailuresReadAsOffline() {
        val hostname = java.net.UnknownHostException(
            """Unable to resolve host "app.exponential.at": No address associated with hostname"""
        )
        assertTrue(isOfflineError(hostname))
        assertEquals(OFFLINE_MESSAGE, trpcErrorMessage(hostname, "Failed to create issue"))

        for (t in listOf(
            java.nio.channels.UnresolvedAddressException(),
            java.net.ConnectException("Failed to connect to /10.0.2.2:3000"),
            java.net.NoRouteToHostException(),
            java.net.PortUnreachableException(),
            java.net.SocketTimeoutException("timeout"),
            io.ktor.client.network.sockets.ConnectTimeoutException("Connect timeout has expired"),
            java.io.IOException("unexpected end of stream"),
        )) {
            assertTrue(t.javaClass.simpleName, isOfflineError(t))
            assertEquals(OFFLINE_MESSAGE, trpcErrorMessage(t, "fallback"))
        }
    }

    // ktor wraps engine exceptions, so the classification has to walk the
    // chain — the top-level throwable is often a generic wrapper.
    @Test
    fun wrappedTransportCauseIsStillOffline() {
        val wrapped = RuntimeException("request failed", java.net.UnknownHostException("no dns"))
        assertTrue(isOfflineError(wrapped))
        assertEquals(OFFLINE_MESSAGE, trpcErrorMessage(wrapped, "fallback"))
    }

    // A tRPC failure came back FROM the server, which proves it was reachable
    // — even a 5xx, and even when some IOException sits in its cause chain.
    @Test
    fun trpcExceptionIsNeverOffline() {
        val error = TrpcException("Something went wrong", HttpStatusCode.InternalServerError)
        assertFalse(isOfflineError(error))
        assertEquals("Something went wrong", trpcErrorMessage(error, "fallback"))
    }

    @Test
    fun nonTransportThrowableIsNotOffline() {
        assertFalse(isOfflineError(IllegalStateException("No active account")))
        assertFalse(isOfflineError(null))
        assertEquals("fallback", trpcErrorMessage(IllegalStateException("x"), "fallback"))
    }

    // The "Fix merge conflicts" gate (EXP-533): a rebase run can only help
    // with a real content conflict, so ONLY the server's 409 opens it.
    @Test
    fun onlyConflictStatusIsAConflict() {
        assertTrue(isConflictError(TrpcException("has merge conflicts", HttpStatusCode.Conflict)))
        assertFalse(
            isConflictError(
                TrpcException("Squash merging is not allowed", HttpStatusCode.PreconditionFailed)
            )
        )
        assertFalse(isConflictError(TrpcException("Not found", HttpStatusCode.NotFound)))
        assertFalse(isConflictError(TrpcException("no status", null)))
        assertFalse(isConflictError(java.net.UnknownHostException("offline")))
        assertFalse(isConflictError(null))
    }

    // TRANSITIONAL (EXP-533): a self-host pinned to a pre-EXP-533 tag still
    // answers a real conflict with 412 + this sentence. Drop with the sniff.
    @Test
    fun legacyPreconditionFailedConflictSentenceStillCounts() {
        assertTrue(
            isConflictError(
                TrpcException(
                    "This branch has merge conflicts with main that must be resolved",
                    HttpStatusCode.PreconditionFailed,
                )
            )
        )
    }
}
