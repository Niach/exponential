package com.exponential.app.navigation

import androidx.compose.runtime.staticCompositionLocalOf

/**
 * EXP-920: where an entity-preview sheet's "Open" lands — one target per
 * detail surface a synced row can have on the phone. The transcript's chips
 * name any of the contract's 18 kinds, so the screen gets ONE navigator
 * rather than a lambda per kind.
 */
sealed interface EntityTarget {
    data class Issue(val id: String) : EntityTarget
    data class Board(val id: String) : EntityTarget
    data class Session(val id: String) : EntityTarget
    data class Workflow(val id: String) : EntityTarget
    data class SupportThread(val id: String) : EntityTarget
    data object Workflows : EntityTarget
    data object Actions : EntityTarget
    data object Automations : EntityTarget
    data object Devices : EntityTarget
    data object TeamSettings : EntityTarget
    data object Inbox : EntityTarget
}

fun interface EntityNavigator {
    fun open(target: EntityTarget)
}

/**
 * Provided ONCE around the nav host with the controller; the default is a
 * no-op so a preview or a test composition still renders the chips, just
 * without anywhere to go.
 */
val LocalEntityNavigator = staticCompositionLocalOf<EntityNavigator> { EntityNavigator { } }
