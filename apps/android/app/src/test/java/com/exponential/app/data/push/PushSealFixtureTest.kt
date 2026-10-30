package com.exponential.app.data.push

import at.exponential.net.ffi.EnetException
import at.exponential.net.ffi.pushKeypairDerive
import at.exponential.net.ffi.pushKeypairGenerate
import at.exponential.net.ffi.pushOpen
import at.exponential.net.ffi.pushSeal
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * EXP-1435: packages/domain-contract/fixtures/push-seal.json replayed through
 * the push-only UniFFI binding (the host build of exponential-net-ffi, loaded
 * by JNA from `jna.library.path`) — the same vectors the Rust core, the web
 * sealer and iOS replay.
 */
class PushSealFixtureTest {
    private val fixture: JsonObject = run {
        val path = System.getProperty("exponential.push.sealFixture")
            ?: error("exponential.push.sealFixture is not set (app/build.gradle.kts)")
        Json.parseToJsonElement(File(path).readText()).jsonObject
    }

    private fun JsonObject.hex(key: String): ByteArray = this[key]!!.jsonPrimitive.content.hexBytes()
    private fun JsonObject.obj(key: String): JsonObject = this[key]!!.jsonObject

    @Test
    fun theVectorOpensToItsPlaintextAndParsesToOpened() {
        val recipient = fixture.obj("recipient")
        val vector = fixture.obj("vector")
        val plaintext = pushOpen(recipient.hex("secret"), vector.hex("sealed"))
        assertEquals(vector["plaintext"]!!.jsonPrimitive.content, plaintext.toString(Charsets.UTF_8))

        val opened = vector.obj("opened")
        val payload = parseSealedPushPayload(plaintext)!!
        assertEquals(opened["title"]!!.jsonPrimitive.content, payload.title)
        assertEquals(opened["body"]!!.jsonPrimitive.content, payload.body)
        assertEquals(opened.obj("data").mapValues { it.value.jsonPrimitive.content }, payload.data)
    }

    @Test
    fun theFcmPathOpensTheBase64UrlValueTheRelayDelivers() {
        val recipient = fixture.obj("recipient")
        val sealed = base64UrlEncode(fixture.obj("vector").hex("sealed"))
        val payload = openSealedPush(sealed) { pushOpen(recipient.hex("secret"), it) }
        assertEquals("Hello", payload?.title)
        assertEquals(mapOf("userId" to "u1"), payload?.data)
    }

    @Test
    fun aRustSealedPayloadOpens() {
        val rust = fixture.obj("rustSealed")
        val plaintext = pushOpen(fixture.obj("recipient").hex("secret"), rust.hex("sealed"))
        assertEquals(rust["plaintext"]!!.jsonPrimitive.content, plaintext.toString(Charsets.UTF_8))
    }

    @Test
    fun deriveMatches() {
        val derive = fixture.obj("derive")
        val pair = pushKeypairDerive(derive.hex("identity"))
        assertArrayEquals(derive.hex("secret"), pair.secret)
        assertArrayEquals(derive.hex("public"), pair.public)
    }

    @Test
    fun everyRefusedCaseThrows() {
        val refused = fixture["refused"]!!.jsonArray
        assertTrue(refused.size >= 9)
        for (case in refused) {
            val c = case.jsonObject
            val name = c["name"]!!.jsonPrimitive.content
            try {
                pushOpen(c.hex("secret"), c.hex("sealed"))
                fail("opened a refused case: $name")
            } catch (_: EnetException) {
            }
            // The FCM path drops it instead of throwing.
            assertNull(name, PushCrypto.open(c.hex("secret"), c.hex("sealed")))
        }
    }

    @Test
    fun sealingToTheLowOrderPublicThrows() {
        try {
            pushSeal(fixture.hex("lowOrderPublic"), "x".toByteArray())
            fail("sealed to a low-order public key")
        } catch (_: EnetException) {
        }
    }

    @Test
    fun aGeneratedKeypairRoundTrips() {
        val keys = PushCrypto.generate()
        assertNotNull(keys)
        assertEquals(32, keys!!.secret.size)
        assertEquals(32, keys.public.size)
        assertEquals(43, keys.publicBase64Url.length)
        val sealed = pushSeal(keys.public, """{"title":"t","body":"b","data":{"type":"x"}}""".toByteArray())
        val payload = openSealedPush(base64UrlEncode(sealed)) { PushCrypto.open(keys.secret, it) }
        assertEquals(SealedPushPayload("t", "b", mapOf("type" to "x")), payload)
        // Another install's key cannot open it.
        assertNull(PushCrypto.open(pushKeypairGenerate().secret, sealed))
    }
}

private fun String.hexBytes(): ByteArray = chunked(2).map { it.toInt(16).toByte() }.toByteArray()
