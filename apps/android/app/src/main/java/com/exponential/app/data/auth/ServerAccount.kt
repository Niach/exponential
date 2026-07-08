package com.exponential.app.data.auth

import java.net.URI
import java.security.MessageDigest
import kotlinx.serialization.Serializable

@Serializable
data class ServerAccount(
    val id: String,
    val instanceUrl: String,
    val token: String? = null,
    val userEmail: String? = null,
    val userName: String? = null,
    val userId: String? = null,
    val isAdmin: Boolean = false,
    // ISO timestamp the user finished onboarding, or null if they haven't. Read
    // from the better-auth session at login (the same source the web app gates
    // on) and persisted so the onboarding gate resolves synchronously at startup.
    val onboardingCompletedAt: String? = null,
    // True once onboardingCompletedAt was actually read from the server. Accounts
    // persisted by builds before the onboarding field existed decode as false and
    // must be treated as already onboarded — only a session read that explicitly
    // reported "not completed" should start the wizard.
    val onboardingKnown: Boolean = false,
    val lastUsedAt: Long = System.currentTimeMillis(),
) {
    // The nav gate: show the first-run wizard only when the server told us
    // onboarding isn't done. Legacy accounts (onboardingKnown=false) never bounce.
    val needsOnboarding: Boolean
        get() = onboardingKnown && onboardingCompletedAt == null

    val displayHost: String
        get() = runCatching { URI(instanceUrl).host }.getOrNull() ?: instanceUrl

    companion object {
        // Pre-login "pending" id: keyed by URL only, before a user is resolved.
        fun makeId(instanceUrl: String): String = hash(instanceUrl)

        // Per-user account id: two users on the same server get DIFFERENT ids —
        // hence different Room DB files, offsets, and workspace selection — so a
        // sign-out/sign-in as another user can never surface the first user's
        // cached data. The URL-only id survives as the pending identity.
        fun makeId(instanceUrl: String, userId: String): String = hash("$instanceUrl\n$userId")

        private fun hash(input: String): String {
            val digest = MessageDigest.getInstance("SHA-256")
                .digest(input.toByteArray(Charsets.UTF_8))
            return digest.take(4).joinToString("") { "%02x".format(it) }
        }
    }
}
