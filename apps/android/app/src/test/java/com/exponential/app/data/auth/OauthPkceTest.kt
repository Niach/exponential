package com.exponential.app.data.auth

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// PKCE derivation for the mobile OAuth handoff (REV-13). The S256 vector is
// RFC 7636 Appendix B — the same pair is asserted by the web
// (mobile-oauth-code.test.ts), iOS (PkceTests), and desktop (login.rs) tests
// so all four implementations provably agree.
class OauthPkceTest {
    @Test
    fun challengeS256MatchesRfc7636Vector() {
        assertEquals(
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
            OauthPkce.challengeS256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        )
    }

    @Test
    fun generateVerifierIsBase64UrlOfExpectedLength() {
        val verifier = OauthPkce.generateVerifier()
        // 32 random bytes → 43 base64url chars, no padding (RFC 7636 §4.1 valid).
        assertEquals(43, verifier.length)
        assertTrue(verifier.matches(Regex("^[A-Za-z0-9_-]+$")))
    }

    @Test
    fun generateVerifierDoesNotRepeat() {
        assertNotEquals(OauthPkce.generateVerifier(), OauthPkce.generateVerifier())
    }

    @Test
    fun challengeIsValidChallengeCharset() {
        val challenge = OauthPkce.challengeS256(OauthPkce.generateVerifier())
        // SHA-256 digest → 43 base64url chars, no padding.
        assertEquals(43, challenge.length)
        assertTrue(challenge.matches(Regex("^[A-Za-z0-9_-]+$")))
    }
}
