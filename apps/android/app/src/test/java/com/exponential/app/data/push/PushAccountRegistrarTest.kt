package com.exponential.app.data.push

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1435: which registration an account posts, and whether the pass may stop retrying it. */
class PushAccountRegistrarTest {
    private var relayUrl: suspend () -> String? = { "https://push.example" }
    private var publicKey: String? = "PUB"
    private var capability: String? = "cap_x"
    private val posted = mutableListOf<SealedRegistration?>()
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

    @Test
    fun aRelayAndACapabilityPostTheSealedRegistration() = runBlocking {
        assertTrue(registrar().register("a", "tok"))
        assertEquals(listOf(SealedRegistration("cap_x", "PUB")), posted)
        assertEquals(listOf("https://push.example a tok"), capabilityCalls)
    }

    @Test
    fun noRelayIsTheBareTokenAndComplete() = runBlocking {
        relayUrl = { null }
        assertTrue(registrar().register("a", "tok"))
        assertEquals(listOf<SealedRegistration?>(null), posted)
        assertTrue(capabilityCalls.isEmpty())
    }

    @Test
    fun noNativeFacadeIsTheBareTokenAndComplete() = runBlocking {
        publicKey = null
        assertTrue(registrar().register("a", "tok"))
        assertEquals(listOf<SealedRegistration?>(null), posted)
        assertTrue(capabilityCalls.isEmpty())
    }

    @Test
    fun aRelayFailureIsTheBareTokenButIncomplete() = runBlocking {
        capability = null
        assertFalse(registrar().register("a", "tok"))
        assertEquals(listOf<SealedRegistration?>(null), posted)
    }

    @Test
    fun aRelayLookupFailureIsTheBareTokenButIncomplete() = runBlocking {
        relayUrl = { throw java.io.IOException("offline") }
        assertFalse(registrar().register("a", "tok"))
        assertEquals(listOf<SealedRegistration?>(null), posted)
    }

    @Test(expected = IllegalStateException::class)
    fun aRegisterFailureThrows(): Unit = runBlocking {
        PushAccountRegistrar(
            relayUrl = { null },
            publicKey = { null },
            capabilityFor = { _, _, _ -> null },
            register = { _, _, _ -> throw IllegalStateException("server blip") },
        ).register("a", "tok")
    }
}
