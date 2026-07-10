package com.exponential.app.data.auth

import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64

/**
 * PKCE for the mobile OAuth handoff (REV-13). The app sends
 * `code_challenge = S256(verifier)` to /api/mobile-oauth-start and keeps the
 * verifier in memory; the `exponential://oauth-return` deep link then carries
 * only a single-use code that MainActivity redeems via
 * POST /api/mobile-oauth-exchange with the verifier. A co-installed app that
 * wins the custom-scheme intent resolution (custom schemes can never be App
 * Links) intercepts a code it cannot redeem — never the session token.
 *
 * Pure JVM (java.util.Base64 needs minSdk 26) so it is unit-testable.
 */
object OauthPkce {
    // 32 random bytes → 43-char base64url verifier (RFC 7636 §4.1 minimum
    // entropy, matches the charset `[A-Za-z0-9-_]` ⊂ unreserved).
    fun generateVerifier(): String {
        val bytes = ByteArray(32)
        SecureRandom().nextBytes(bytes)
        return Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
    }

    // RFC 7636 §4.2: challenge = base64url_no_pad(SHA-256(ASCII(verifier))).
    fun challengeS256(verifier: String): String {
        val digest = MessageDigest.getInstance("SHA-256").digest(verifier.toByteArray(Charsets.US_ASCII))
        return Base64.getUrlEncoder().withoutPadding().encodeToString(digest)
    }
}
