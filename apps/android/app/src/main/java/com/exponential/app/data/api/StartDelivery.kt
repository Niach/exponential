package com.exponential.app.data.api

import io.ktor.client.plugins.HttpRequestTimeoutException

/**
 * Recheck R3 (feedback-states.json `starting.maybeDelivered`): whether a
 * FAILED `steer.startSession` may still have reached the device. A request
 * whose fate is unknown (a network or transport error, a client-side timeout,
 * the server's `relay_timeout` answer) may have: the host shows the failure
 * toast with Retry (same `startId`) AND follows the start quietly for
 * `starting.watchMs`, so a late session still lands. A response with a
 * definite error (any 4xx, or a 5xx whose error body is not `relay_timeout`)
 * is a definite failure: toast + Retry, no follower. One rule ×4 (web, the
 * desktop's `may_have_reached_device`, iOS `StartDelivery`).
 */
object StartDelivery {

    fun maybeDelivered(error: Throwable): Boolean {
        var t: Throwable? = error
        var depth = 0
        while (t != null && depth < 16) {
            val current: Throwable = t
            when (current) {
                is TrpcException -> {
                    // No status = never sent (no instance URL) or a local refusal.
                    val status = current.status?.value ?: return false
                    // A 5xx with no tRPC error body (a proxy's) says nothing
                    // about the start; the body's code is what proves one.
                    val bodyMessage = if (current.code != null) current.message else null
                    return maybeDelivered(status, bodyMessage)
                }
                is HttpRequestTimeoutException -> return true
            }
            val next = current.cause
            t = if (next === current) null else next
            depth++
        }
        return isOfflineError(error)
    }

    /**
     * An HTTP answer: a 4xx refused it; a 5xx refused it only when its error
     * body names something other than the relay timing out.
     */
    fun maybeDelivered(status: Int, message: String?): Boolean {
        if (status < 500) return false
        if (message.isNullOrEmpty()) return true
        return isRelayTimeout(message)
    }

    /**
     * The server's relay timeout (REV-34): the relay's `relay_timeout` reason,
     * which `steer.startSession` reports as the relay's 504.
     */
    fun isRelayTimeout(message: String): Boolean =
        "relay_timeout" in message || "Steer relay error (504)" in message
}
