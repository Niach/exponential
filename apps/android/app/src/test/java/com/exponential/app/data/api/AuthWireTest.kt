package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the EXP-857 login wire helpers: the passkey challenge-cookie replay
 * (the app's HTTP client keeps no cookie jar, so the verify call carries them
 * by hand), the verify body shape, and the one-time code error copy.
 */
class AuthWireTest {

    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun setCookiePairsKeepsNameValueAndDropsAttributes() {
        val pairs = AuthWire.setCookiePairs(
            listOf(
                "__Secure-better-auth.better-auth-passkey=abc.def; Path=/; HttpOnly; Secure; SameSite=Lax",
                "better-auth.state=xyz; Max-Age=600; Path=/",
            ),
        )
        assertEquals(
            listOf("__Secure-better-auth.better-auth-passkey=abc.def", "better-auth.state=xyz"),
            pairs,
        )
    }

    @Test
    fun setCookiePairsSkipsGarbage() {
        val pairs = AuthWire.setCookiePairs(listOf("", "Path=/; HttpOnly", "=orphan", " a=b ; Path=/"))
        // `Path=/` reads as a name=value pair on purpose — a request Cookie
        // header is name=value only, and guessing which names are attributes
        // would drop real cookies. Only blank names are refused.
        assertEquals(listOf("Path=/", "a=b"), pairs)
    }

    @Test
    fun cookieHeaderJoinsWithSemicolons() {
        assertEquals(
            "__Secure-x=1; y=2",
            AuthWire.cookieHeader(listOf("__Secure-x=1", "y=2")),
        )
        assertEquals("", AuthWire.cookieHeader(emptyList()))
    }

    @Test
    fun passkeyVerifyBodyEmbedsTheAssertionAsAnObject() {
        val assertion = """
            {"id":"cred-1","rawId":"cred-1","type":"public-key",
             "response":{"clientDataJSON":"cdj","authenticatorData":"ad","signature":"sig"},
             "clientExtensionResults":{}}
        """.trimIndent()

        val body = AuthWire.passkeyVerifyBody(json, assertion)
        val response = body["response"]
        assertTrue("response must be an object, not a string", response is JsonObject)
        assertEquals("cred-1", response!!.jsonObject["id"]!!.jsonPrimitive.content)
        assertEquals(
            "sig",
            response.jsonObject["response"]!!.jsonObject["signature"]!!.jsonPrimitive.content,
        )
        // And it must serialize back as a nested object.
        assertTrue(json.encodeToString(JsonObject.serializer(), body).contains("\"response\":{\"id\":"))
    }

    @Test
    fun passkeyVerifyBodyRejectsANonObjectAssertion() {
        assertThrows(Exception::class.java) {
            AuthWire.passkeyVerifyBody(json, "\"not-an-object\"")
        }
    }

    @Test
    fun otpErrorMessageUsesThePreferredCopyPerCode() {
        assertEquals(
            "That code is not right. Check the email and try again.",
            AuthWire.otpErrorMessage("INVALID_OTP", "Invalid OTP"),
        )
        assertEquals(
            "That code expired. Request a new one.",
            AuthWire.otpErrorMessage("OTP_EXPIRED", "OTP expired"),
        )
        assertEquals(
            "Too many attempts. Request a new code.",
            AuthWire.otpErrorMessage("TOO_MANY_ATTEMPTS", "Too many attempts"),
        )
    }

    @Test
    fun otpErrorMessageFallsBackToTheServerMessage() {
        assertEquals("Signups are closed", AuthWire.otpErrorMessage("SIGNUP_DISABLED", "Signups are closed"))
        assertEquals(
            "Couldn't verify that code. Please try again.",
            AuthWire.otpErrorMessage(null, null),
        )
        assertEquals(
            "Couldn't verify that code. Please try again.",
            AuthWire.otpErrorMessage("WHATEVER", "   "),
        )
    }

    @Test
    fun sendCodeErrorMessagePrefersTheServerThenTheStatus() {
        assertEquals("Invalid email", AuthWire.sendCodeErrorMessage(400, "Invalid email"))
        assertEquals(
            "Too many requests. Wait a moment and try again.",
            AuthWire.sendCodeErrorMessage(429, null),
        )
        assertEquals("Couldn't send the code (HTTP 500)", AuthWire.sendCodeErrorMessage(500, null))
    }
}
