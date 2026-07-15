package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * App-wide latch for the client-upgrade-required signal (EXP-104). The shared
 * HTTP client raises it when the server answers HTTP 426 (this build is below
 * the configured minimum); the app-level state flow then swaps in a blocking
 * "Update required" screen. First trigger wins — later 426s (including from
 * shape polls that are already unwinding) are ignored so the gate can't flicker.
 */
@Singleton
class UpdateGate @Inject constructor() {

    data class UpgradeInfo(val min: String?, val latest: String?)

    private val _state = MutableStateFlow<UpgradeInfo?>(null)
    val state: StateFlow<UpgradeInfo?> = _state.asStateFlow()

    fun trigger(info: UpgradeInfo) {
        // Idempotent: keep the first signal; the gate is terminal for this run.
        _state.compareAndSet(null, info)
    }
}
