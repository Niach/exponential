package com.exponential.app.data.enet

import java.io.File
import java.security.KeyFactory
import java.security.Signature
import java.security.spec.X509EncodedKeySpec
import java.time.Instant
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

internal fun String.hexBytes(): ByteArray = chunked(2).map { it.toInt(16).toByte() }.toByteArray()

/** The server's check (apps/web installs.ts), with the JDK's ed25519. */
internal fun verifiesAgainstEndpoint(endpointId: String, message: String, signatureB64Url: String): Boolean {
    val spki = "302a300506032b6570032100".hexBytes() + endpointId.hexBytes()
    val key = KeyFactory.getInstance("Ed25519").generatePublic(X509EncodedKeySpec(spki))
    return Signature.getInstance("Ed25519").run {
        initVerify(key)
        update(message.toByteArray(Charsets.UTF_8))
        verify(java.util.Base64.getUrlDecoder().decode(signatureB64Url))
    }
}

/**
 * EXP-1457: packages/domain-contract/fixtures/install-register.json through
 * the eNet facade's host build (JNA) — the bytes the Rust facade and the
 * server replay too.
 */
class InstallRegistrationTest {
    private val fixture: JsonObject = run {
        val path = System.getProperty("exponential.enet.installFixture")
            ?: error("exponential.enet.installFixture is not set (app/build.gradle.kts)")
        Json.parseToJsonElement(File(path).readText()).jsonObject
    }

    private fun str(key: String) = fixture[key]!!.jsonPrimitive.content

    @Test
    fun signsTheFixtureVectorByteExact() {
        val input = signedRegistration(
            crypto = EnetFfi,
            identity = str("identity").hexBytes(),
            userId = str("userId"),
            signedAt = Instant.parse(str("signedAt")),
            label = "Pixel 9",
            platform = "Android 16",
        )
        assertEquals("android", input.kind)
        assertEquals(str("endpointId"), input.endpointId)
        assertEquals(str("x25519Public"), input.x25519Public)
        assertEquals(str("signedAt"), input.signedAt)
        assertEquals(str("message"), installMessage(str("userId"), input.endpointId, input.x25519Public, input.signedAt))
        assertEquals(str("signature"), input.signature)
        assertTrue(verifiesAgainstEndpoint(input.endpointId, str("message"), input.signature))
    }

    @Test
    fun aFreshIdentitySignsWhatTheServerVerifies() {
        val identity = EnetFfi.generateIdentity()
        val input = signedRegistration(EnetFfi, identity, "u-42", Instant.now(), "x".repeat(80), "y".repeat(40))
        assertTrue(input.endpointId.matches(Regex("^[0-9a-f]{64}$")))
        assertTrue(input.x25519Public.matches(Regex("^[A-Za-z0-9_-]{43}$")))
        assertEquals(64, input.label!!.length)
        assertEquals(32, input.platform!!.length)
        val message = installMessage("u-42", input.endpointId, input.x25519Public, input.signedAt)
        assertTrue(verifiesAgainstEndpoint(input.endpointId, message, input.signature))
    }

    @Test
    fun signedAtIsIsoWithMillis() {
        assertEquals("2026-10-30T12:00:00.000Z", isoMillis(Instant.parse("2026-10-30T12:00:00Z")))
        assertEquals("2026-10-30T12:00:00.123Z", isoMillis(Instant.parse("2026-10-30T12:00:00.123456Z")))
    }

    @Test
    fun accessDiffNeverAdmitsSelf() {
        val diff = accessDiff(current = setOf("a", "b"), desired = setOf("b", "c", "self"), self = "self")
        assertEquals(setOf("c"), diff.allow)
        assertEquals(setOf("a"), diff.revoke)
    }
}
