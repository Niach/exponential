package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the `/api/auth-config` decode (server contract:
 * apps/web/src/lib/auth/config.ts) and the browser hand-off URLs the login
 * screen builds from it. The signup/reset flags drive the "Create account" and
 * "Forgot password?" affordances, so a server that omits them must decode to
 * false rather than blow up or offer a dead link.
 */
class AuthConfigDecodingTest {

    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun decodesSignupAndResetFlags() {
        val payload = """
            {
              "passwordEnabled": true,
              "signupEnabled": true,
              "passwordResetEnabled": true,
              "oidcProviders": [{ "id": "authentik", "name": "Authentik" }],
              "googleLoginEnabled": false,
              "appleLoginEnabled": false,
              "githubEnabled": true
            }
        """.trimIndent()

        val config = json.decodeFromString<AuthConfig>(payload)
        assertTrue(config.signupEnabled)
        assertTrue(config.passwordResetEnabled)
        assertEquals("authentik", config.oidcProviders.single().id)
    }

    /** EXP-857: the code + passkey flags gate their two buttons. */
    @Test
    fun decodesEmailOtpAndPasskeyFlags() {
        val payload = """
            {
              "passwordEnabled": false,
              "signupEnabled": true,
              "oidcProviders": [],
              "emailOtpEnabled": true,
              "passkeyEnabled": true
            }
        """.trimIndent()

        val config = json.decodeFromString<AuthConfig>(payload)
        assertFalse(config.passwordEnabled)
        assertTrue(config.emailOtpEnabled)
        assertTrue(config.passkeyEnabled)
    }

    @Test
    fun missingFlagsDefaultToFalse() {
        val payload = """
            {
              "passwordEnabled": true,
              "oidcProviders": [],
              "googleLoginEnabled": false
            }
        """.trimIndent()

        val config = json.decodeFromString<AuthConfig>(payload)
        assertTrue(config.passwordEnabled)
        assertFalse(config.signupEnabled)
        assertFalse(config.passwordResetEnabled)
        assertFalse(config.appleLoginEnabled)
        // A pre-EXP-857 server publishes neither flag — both must read as off
        // rather than offering a login path the instance can't serve.
        assertFalse(config.emailOtpEnabled)
        assertFalse(config.passkeyEnabled)
    }

    @Test
    fun webHandoffUrlsTrimTrailingSlash() {
        assertEquals(
            "https://app.exponential.at/auth/register?ref=android-app",
            AuthWebUrls.register("https://app.exponential.at/"),
        )
        assertEquals(
            "https://app.exponential.at/auth/forgot-password",
            AuthWebUrls.forgotPassword("https://app.exponential.at"),
        )
    }
}
