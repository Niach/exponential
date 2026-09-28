package com.exponential.app.ui.spike

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * VAPP-4 spike: the `exp.devScreen` intent extra (debug builds). MainActivity
 * publishes the requested screen here; AppNavHost navigates to it once the
 * signed-in graph is up, and renders it as a full-screen overlay while no
 * account is signed in (so the spike runs without a backend).
 *
 *   adb shell am start -n at.exponential/com.exponential.app.MainActivity \
 *     --es exp.devScreen kitchen-sink        # | kitchen-sink-bench | kitchen-sink-rtl
 */
object DevScreens {
    const val EXTRA = "exp.devScreen"
    const val ROUTE_PATTERN = "kitchen-sink?bench={bench}&rtl={rtl}"
    const val BENCH_NODES = 200

    /** A parsed request: [bench] = node count of the bench tree, null = the fixture. */
    data class Request(val bench: Int?, val rtl: Boolean) {
        val route: String get() = "kitchen-sink?bench=${bench ?: 0}&rtl=$rtl"
    }

    /** The host-side per-pass measure memo (VappSurface); the bench test flips it. */
    @Volatile var hostMemo: Boolean = true

    private val _request = MutableStateFlow<Request?>(null)
    val request: StateFlow<Request?> = _request.asStateFlow()

    fun parse(value: String): Request? = when (value.trim()) {
        "kitchen-sink" -> Request(bench = null, rtl = false)
        "kitchen-sink-rtl" -> Request(bench = null, rtl = true)
        "kitchen-sink-bench" -> Request(bench = BENCH_NODES, rtl = false)
        else -> null
    }

    fun open(value: String): Boolean {
        val parsed = parse(value) ?: return false
        _request.value = parsed
        return true
    }

    fun consume() {
        _request.value = null
    }
}
