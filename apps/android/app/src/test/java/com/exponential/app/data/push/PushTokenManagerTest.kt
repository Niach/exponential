package com.exponential.app.data.push

import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * REV2-45: push registration used to be one-shot — a transient failure left
 * that account silently push-dead until the next cold start, and a register in
 * flight when sign-out ran resurrected the server row its unregister had just
 * deleted. These cover the reconcile/backoff/compensating-unregister semantics
 * the iOS client already had (Data/Push/PushTokenManager.swift).
 */
class PushTokenManagerTest {

    private val state = PushRegistrationState()
    private var signedIn = setOf("a")
    private var deviceToken: String? = "tok-1"
    private val registerCalls = mutableListOf<Pair<String, String>>()
    private val unregisterCalls = mutableListOf<Pair<String, String>>()
    private var onRegister: suspend (String) -> Unit = {}

    private fun reconciler(registerTimeoutMs: Long = 10_000L) = PushRegistrationReconciler(
        state = state,
        signedInAccounts = { signedIn },
        deviceToken = { deviceToken },
        register = { accountId, token ->
            registerCalls += accountId to token
            onRegister(accountId)
        },
        unregister = { accountId, token -> unregisterCalls += accountId to token },
        registerTimeoutMs = registerTimeoutMs,
    )

    @Test
    fun everySignedInAccountIsRegisteredOnceThenTheLoopGoesQuiet() = runBlocking {
        signedIn = setOf("b", "a")
        val reconciler = reconciler()
        assertFalse(reconciler.reconcile())
        assertEquals(listOf("a" to "tok-1", "b" to "tok-1"), registerCalls)
        // Nothing outstanding: a second pass posts nothing and asks for no retry.
        assertFalse(reconciler.reconcile())
        assertEquals(2, registerCalls.size)
    }

    @Test
    fun aFailedRegisterIsRetriedOnTheNextPass() = runBlocking {
        onRegister = { throw IllegalStateException("server blip") }
        val reconciler = reconciler()
        assertTrue(reconciler.reconcile())
        assertEquals(listOf("a" to "tok-1"), registerCalls)

        onRegister = {}
        assertFalse(reconciler.reconcile())
        assertEquals(listOf("a" to "tok-1", "a" to "tok-1"), registerCalls)
    }

    @Test
    fun aStuckRegisterTimesOutInsteadOfStarvingTheOtherAccounts() = runBlocking {
        signedIn = setOf("a", "b")
        onRegister = { accountId -> if (accountId == "a") delay(10_000) }
        val reconciler = reconciler(registerTimeoutMs = 50)
        assertTrue(reconciler.reconcile())
        // "b" was still reached, and "a" is not acknowledged.
        assertEquals(listOf("a" to "tok-1", "b" to "tok-1"), registerCalls)

        onRegister = {}
        assertFalse(reconciler.reconcile())
        assertEquals(3, registerCalls.size)
        assertEquals("a" to "tok-1", registerCalls.last())
    }

    @Test
    fun anFcmTokenFailureRetriesInsteadOfDroppingThePass() = runBlocking {
        deviceToken = null
        val reconciler = reconciler()
        assertTrue(reconciler.reconcile())
        assertTrue(registerCalls.isEmpty())

        deviceToken = "tok-1"
        assertFalse(reconciler.reconcile())
        assertEquals(listOf("a" to "tok-1"), registerCalls)
    }

    @Test
    fun aRotatedTokenIsPostedForEveryAccount() = runBlocking {
        signedIn = setOf("a", "b")
        val reconciler = reconciler()
        assertFalse(reconciler.reconcile())

        // The rotation path: FcmService.onNewToken stashes the new token, which
        // drops every acknowledgement, and the loop posts it again.
        state.setToken("tok-2")
        deviceToken = "tok-2"
        assertFalse(reconciler.reconcile())
        assertEquals(listOf("a" to "tok-2", "b" to "tok-2"), registerCalls.takeLast(2))
    }

    @Test
    fun aSignOutDuringAnInFlightRegisterCompensates() = runBlocking {
        // Sign-out's unregister lands while this register is on the wire: the
        // server row it recreates is one nothing else can delete afterwards.
        onRegister = { accountId -> state.suppress(accountId) }
        val reconciler = reconciler()
        assertFalse(reconciler.reconcile())
        assertEquals(listOf("a" to "tok-1"), registerCalls)
        assertEquals(listOf("a" to "tok-1"), unregisterCalls)

        // Still signed out: the loop must not re-register it.
        onRegister = {}
        assertFalse(reconciler.reconcile())
        assertEquals(1, registerCalls.size)
    }

    @Test
    fun aReLoginAfterSignOutRegistersAgain() = runBlocking {
        val reconciler = reconciler()
        state.suppress("a")
        assertFalse(reconciler.reconcile())
        assertTrue(registerCalls.isEmpty())

        // Credentials dropped, then the same account signs back in.
        signedIn = emptySet()
        assertFalse(reconciler.reconcile())
        signedIn = setOf("a")
        assertFalse(reconciler.reconcile())
        assertEquals(listOf("a" to "tok-1"), registerCalls)
    }

    @Test
    fun suppressionSurvivesTheAccountVanishingMidFlight() {
        state.setToken("tok-1")
        assertTrue(state.beginRegister("a", "tok-1"))
        state.suppress("a")
        // The account is gone from the account store already; suppression must
        // outlive it until the in-flight register reports back, or the
        // compensating unregister never fires.
        assertTrue(state.pending(emptySet()).isEmpty())
        assertTrue(state.completeRegister("a", "tok-1"))
    }

    @Test
    fun aTokenRotationMidFlightIsNotAcknowledged() {
        state.setToken("tok-1")
        assertTrue(state.beginRegister("a", "tok-1"))
        state.setToken("tok-2")
        // The call posted the OLD token; acknowledging it would skip posting
        // the new one, and FCM has already killed the old one.
        assertFalse(state.completeRegister("a", "tok-1"))
        assertEquals(listOf("a"), state.pending(setOf("a")))
        assertFalse(state.beginRegister("a", "tok-1"))
    }

    @Test
    fun pendingPrunesAccountsThatAreGone() {
        state.setToken("tok-1")
        assertEquals(listOf("a", "b"), state.pending(setOf("a", "b")))
        assertTrue(state.beginRegister("a", "tok-1"))
        assertFalse(state.completeRegister("a", "tok-1"))
        assertEquals(listOf("b"), state.pending(setOf("a", "b")))
        // "a" signs out and back in: its acknowledgement must not survive.
        assertEquals(listOf("b"), state.pending(setOf("b")))
        assertEquals(listOf("a", "b"), state.pending(setOf("a", "b")))
    }

    @Test
    fun retriesBackOffExponentiallyAndResetOnSuccess() {
        assertEquals(2 * PUSH_BASE_RETRY_MS, nextPushRetryMs(PUSH_BASE_RETRY_MS, failed = true))
        assertEquals(PUSH_MAX_RETRY_MS, nextPushRetryMs(PUSH_MAX_RETRY_MS, failed = true))
        assertEquals(PUSH_MAX_RETRY_MS, nextPushRetryMs(PUSH_MAX_RETRY_MS / 2 + 1, failed = true))
        assertEquals(PUSH_BASE_RETRY_MS, nextPushRetryMs(PUSH_MAX_RETRY_MS, failed = false))
    }
}
