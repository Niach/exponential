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
    val lastUsedAt: Long = System.currentTimeMillis(),
) {
    val displayHost: String
        get() = runCatching { URI(instanceUrl).host }.getOrNull() ?: instanceUrl

    companion object {
        fun makeId(instanceUrl: String): String {
            val digest = MessageDigest.getInstance("SHA-256")
                .digest(instanceUrl.toByteArray(Charsets.UTF_8))
            return digest.take(4).joinToString("") { "%02x".format(it) }
        }
    }
}
