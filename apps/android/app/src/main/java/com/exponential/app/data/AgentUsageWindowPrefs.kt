package com.exponential.app.data

import com.exponential.app.data.auth.SecureStore
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * EXP-484: which usage window a user last picked for an agent — a purely
 * LOCAL preference (the desktop, web and iOS each keep their own; nothing is
 * synced), so a collapsed bar keeps showing the window that person cares
 * about instead of whichever one happens to be busiest.
 *
 * Keyed by agent, not by device: the same person watching the same agent on
 * two machines wants the same window. An unknown/no-longer-reported key just
 * falls back to the busiest window (see [AgentUsagePresentation.selectWindow]).
 *
 * SecureStore isn't observable, so — the TeamSelection.rememberLastBoard idiom
 * — a version counter lets reactive readers re-read after every write.
 */
@Singleton
class AgentUsageWindowPrefs @Inject constructor(
    private val secureStore: SecureStore,
) {
    private val _version = MutableStateFlow(0)
    val version: StateFlow<Int> = _version.asStateFlow()

    fun read(agent: String): String? =
        if (agent.isBlank()) null else secureStore.get(prefKey(agent))

    fun remember(agent: String, key: String) {
        if (agent.isBlank() || key.isBlank()) return
        secureStore.set(prefKey(agent), key)
        _version.value += 1
    }

    private fun prefKey(agent: String) = "agent_usage_window_$agent"
}
