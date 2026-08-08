package com.exponential.app.data.auth

import android.util.Log
import com.exponential.app.data.api.UsersApi
import java.util.TimeZone
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.math.min
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/**
 * EXP-452: stamps the device's IANA timezone on every signed-in account, once
 * per process (`users.setTimezone` with `onlyIfUnset` — an explicit pick in
 * web/desktop settings always wins). The daily digest's send hour is read in
 * `users.timezone`; web and desktop claim it at sign-in, but an account that
 * only ever signed in on mobile stayed NULL and had its digest silently
 * scheduled on UTC's clock (a 10:00 delivery for a "8:00" German account).
 * Follows PushTokenManager's shape: an account-set change wakes one claim
 * pass, failed passes back off exponentially, and the steady state (every
 * account acknowledged) generates no network traffic at all.
 */
@Singleton
class TimezoneClaimer @Inject constructor(
    private val auth: AuthRepository,
    private val api: UsersApi,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    // Accounts whose claim the server acknowledged this process. Never
    // cleared: the zone is claim-once by design (`onlyIfUnset`), so a
    // re-claim after an account switch would be a no-op anyway.
    private val claimed = mutableSetOf<String>()

    // CONFLATED: a burst of account changes collapses into one pass.
    private val wake = Channel<Unit>(Channel.CONFLATED)

    fun start() {
        scope.launch {
            auth.accounts
                .map { accounts -> accounts.filter { it.token != null }.map { it.id }.toSet() }
                .distinctUntilChanged()
                .collect { wake.trySend(Unit) }
        }
        scope.launch { claimLoop() }
    }

    /**
     * Parks until an account change (or the retry window) wakes it, then
     * claims for every signed-in account not yet acknowledged.
     */
    private suspend fun claimLoop() {
        var retryMs = BASE_RETRY_MS
        var retry = false
        while (true) {
            if (retry) withTimeoutOrNull(retryMs) { wake.receive() } else wake.receive()
            retry = claimPending()
            retryMs = if (retry) min(retryMs * 2, MAX_RETRY_MS) else BASE_RETRY_MS
        }
    }

    /** Returns true when a claim failed, i.e. the loop should retry. */
    private suspend fun claimPending(): Boolean {
        val timezone = TimeZone.getDefault().id ?: return false
        val pending = synchronized(claimed) {
            auth.accounts.value.filter { it.token != null }.map { it.id }.toSet() - claimed
        }
        var retry = false
        for (accountId in pending.sorted()) {
            try {
                api.setTimezone(accountId, timezone, onlyIfUnset = true)
                synchronized(claimed) { claimed.add(accountId) }
            } catch (err: Throwable) {
                // Best-effort — a missed claim (offline, an older server
                // without the route) just means the digest reads UTC until
                // the next successful pass or process start.
                retry = true
                Log.w(TAG, "Timezone claim failed: ${err.message}")
            }
        }
        return retry
    }

    private companion object {
        const val TAG = "TimezoneClaimer"
        const val BASE_RETRY_MS = 5_000L
        const val MAX_RETRY_MS = 300_000L
    }
}
