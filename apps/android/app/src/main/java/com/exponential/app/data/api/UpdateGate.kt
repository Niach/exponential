package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * Per-INSTANCE latch for the client-upgrade-required signal (EXP-104). The
 * shared HTTP client raises it when a server answers HTTP 426 (this build is
 * below that server's configured minimum).
 *
 * Keyed by instance origin, never process-global (REV2-18): every signed-in
 * account keeps its own shape pipelines polling its own server in the
 * background, so one self-hosted instance with a `CLIENT_MIN_VERSION_ANDROID`
 * above the installed build used to lock the user out of every other account
 * with no in-app escape. The blocking screen now shows only while the ACTIVE
 * account's instance is gated; a background account's 426 stops that account's
 * sync and surfaces a banner instead. First trigger per origin wins — later
 * 426s (shape polls already unwinding) are ignored so the gate can't flicker.
 */
@Singleton
class UpdateGate @Inject constructor() {

    data class UpgradeInfo(val min: String?, val latest: String?)

    private val _gated = MutableStateFlow<Map<String, UpgradeInfo>>(emptyMap())

    /** Gated instance origins ([originKey]) → that server's advertised versions. */
    val gated: StateFlow<Map<String, UpgradeInfo>> = _gated.asStateFlow()

    fun trigger(instanceUrl: String, info: UpgradeInfo) {
        val key = originKey(instanceUrl) ?: return
        _gated.update { if (it.containsKey(key)) it else it + (key to info) }
    }

    /**
     * Drop a server's latch — its last account was removed, so the next launch
     * of that server (re-adding it) starts from a clean signal instead of a
     * gate no request backs anymore.
     */
    fun clear(instanceUrl: String) {
        val key = originKey(instanceUrl) ?: return
        _gated.update { it - key }
    }

    companion object {
        /**
         * `scheme://host[:port]`, lowercased with the scheme's default port
         * dropped — the join key between a 426 response's URL and a stored
         * account's instance URL. Hand-rolled rather than `android.net.Uri` so
         * both sides of that join normalize identically (and so it stays a
         * pure, unit-testable function).
         */
        fun originKey(url: String): String? {
            val trimmed = url.trim()
            if (trimmed.isEmpty()) return null
            val schemeEnd = trimmed.indexOf("://")
            // Bare hosts (an instance URL typed without a scheme) normalize the
            // same way AuthRepository does: https.
            val scheme =
                if (schemeEnd > 0) trimmed.substring(0, schemeEnd).lowercase() else "https"
            val rest = if (schemeEnd > 0) trimmed.substring(schemeEnd + 3) else trimmed
            val authority = rest
                .takeWhile { it != '/' && it != '?' && it != '#' }
                .substringAfterLast('@') // drop any userinfo
                .lowercase()
            if (authority.isEmpty()) return null

            // IPv6 literals keep their brackets; the port (if any) follows `]`.
            val hostEnd = if (authority.startsWith("[")) authority.indexOf(']') + 1 else 0
            val colon = authority.indexOf(':', startIndex = hostEnd)
            val host = if (colon >= 0) authority.substring(0, colon) else authority
            val port = if (colon >= 0) authority.substring(colon + 1) else null
            if (host.isEmpty() || (port != null && port.toIntOrNull() == null)) return null

            val defaultPort = when (scheme) {
                "https" -> "443"
                "http" -> "80"
                else -> null
            }
            return if (port == null || port == defaultPort) "$scheme://$host"
            else "$scheme://$host:$port"
        }
    }
}
