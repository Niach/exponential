package com.exponential.app.data.push

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * EXP-1435 / NEW-14: which registration an account posts. Only sealed ones:
 * an unusable relay posts nothing and throws, so the reconcile loop retries.
 */
class PushAccountRegistrarTest {
    private var relayUrl: suspend () -> String? = { "https://push.example" }
    private var publicKey: String? = "PUB"
    private var capability: String? = "cap_x"
    private val posted = mutableListOf<SealedRegistration>()
    private val capabilityCalls = mutableListOf<String>()

    private fun registrar() = PushAccountRegistrar(
        relayUrl = { relayUrl() },
        publicKey = { publicKey },
        capabilityFor = { url, accountId, token ->
            capabilityCalls += "$url $accountId $token"
            capability
        },
        register = { _, _, sealed -> posted += sealed },
    )

    private fun assertUnavailable() = runBlocking {
        try {
            registrar().register("a", "tok")
            fail("expected PushRelayUnavailable")
        } catch (_: PushRelayUnavailable) {
        }
        // No bare-token fallback: nothing reaches the server.
        assertTrue(posted.isEmpty())
    }

    @Test
    fun aRelayAndACapabilityPostTheSealedRegistration() = runBlocking {
        registrar().register("a", "tok")
        assertEquals(listOf(SealedRegistration("cap_x", "PUB")), posted)
        assertEquals(listOf("https://push.example a tok"), capabilityCalls)
    }

    @Test
    fun noRelayPostsNothingAndIsDone() = runBlocking {
        // The server cannot push without a relay: nothing to register.
        relayUrl = { null }
        registrar().register("a", "tok")
        assertTrue(posted.isEmpty())
        assertTrue(capabilityCalls.isEmpty())
    }

    @Test
    fun noKeypairPostsNothingAndRetries() {
        publicKey = null
        assertUnavailable()
        assertTrue(capabilityCalls.isEmpty())
    }

    @Test
    fun aRelayFailurePostsNothingAndRetries() {
        capability = null
        assertUnavailable()
    }

    @Test
    fun aRelayLookupFailurePostsNothingAndRetries() {
        relayUrl = { throw java.io.IOException("offline") }
        assertUnavailable()
    }

    @Test(expected = IllegalStateException::class)
    fun aRegisterFailureThrows(): Unit = runBlocking {
        PushAccountRegistrar(
            relayUrl = { "https://push.example" },
            publicKey = { "PUB" },
            capabilityFor = { _, _, _ -> "cap_x" },
            register = { _, _, _ -> throw IllegalStateException("server blip") },
        ).register("a", "tok")
    }
}
