package com.exponential.app.navigation

import androidx.navigation.NavHostController
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * #EXP-2109 (sidebar-tree.json `phone`): a team has no page of its own; every
 * link to one lands on the Teams tab with that team unfolded and scrolled to.
 */
object TeamsTab {
    private val _landing = MutableStateFlow<String?>(null)

    /** The team to land on; consumed by the Teams screen. */
    val landing: StateFlow<String?> = _landing

    fun land(teamId: String) {
        _landing.value = teamId
    }

    fun consume(teamId: String) {
        _landing.compareAndSet(teamId, null)
    }
}

/** The Teams tab's root on [teamId]. */
fun NavHostController.openTeam(teamId: String, marker: TabSwitchMarker? = null) {
    TeamsTab.land(teamId)
    selectTab(MainTabs.TEAMS, marker, restore = false)
}
